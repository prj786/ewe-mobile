//! The phone's address book.
//!
//! KDE Connect's contacts plugin does not hand contacts over D-Bus: it writes
//! one vCard per contact into `~/.local/share/kpeoplevcard/kdeconnect-<id>/`
//! and says "synchronized". Android exports vCard 2.1 — parameters without
//! `TYPE=`, names as `CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE`, photos as
//! folded BASE64 — so this reader takes 2.1 and 3.0 alike. Read-only: the
//! cache is the daemon's.

use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Default, Clone, Serialize, PartialEq)]
pub struct Phone {
    pub number: String,
    /// "mobile", "home", "work" or "" — what the phone called this number
    pub kind: String,
}

#[derive(Debug, Default, Clone, Serialize, PartialEq)]
pub struct Contact {
    pub uid: String,
    pub name: String,
    pub phones: Vec<Phone>,
    pub emails: Vec<String>,
    pub org: String,
    pub note: String,
    #[serde(rename = "hasPhoto")]
    pub has_photo: bool,
}

/// Where the daemon keeps one device's cards.
pub fn dir_for(device: &str) -> PathBuf {
    crate::tools::data_home()
        .join("kpeoplevcard")
        .join(format!("kdeconnect-{device}"))
}

/// One logical line of a card: `NAME;PARAM;PARAM=x:value`.
struct Line {
    name: String,
    params: Vec<String>,
    value: String,
}

impl Line {
    fn has(&self, needle: &str) -> bool {
        self.params.iter().any(|p| {
            p.eq_ignore_ascii_case(needle)
                || p.split_once('=').is_some_and(|(_, v)| {
                    v.trim_matches('"')
                        .split(',')
                        .any(|x| x.eq_ignore_ascii_case(needle))
                })
        })
    }
    fn param(&self, key: &str) -> Option<&str> {
        self.params.iter().find_map(|p| {
            p.split_once('=')
                .filter(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v)
        })
    }
}

/// Unfold a card into logical lines: a line that starts with a space or a
/// tab continues the one before (3.0 folding, and 2.1's BASE64 blocks); a
/// quoted-printable line that ends in `=` continues on the next.
fn logical_lines(text: &str) -> Vec<Line> {
    let mut raw: Vec<String> = Vec::new();
    let mut soft = false; // the previous line ended in a quoted-printable "="
    for l in text.lines() {
        let l = l.strip_suffix('\r').unwrap_or(l);
        if soft {
            if let Some(last) = raw.last_mut() {
                last.push_str(l);
            }
        } else if l.starts_with(' ') || l.starts_with('\t') {
            if let Some(last) = raw.last_mut() {
                last.push_str(&l[1..]);
            }
        } else {
            raw.push(l.to_string());
        }
        soft = false;
        if let Some(last) = raw.last_mut() {
            let qp = last
                .split_once(':')
                .is_some_and(|(head, _)| head.to_ascii_uppercase().contains("QUOTED-PRINTABLE"));
            if qp && last.ends_with('=') {
                last.pop();
                soft = true;
            }
        }
    }
    raw.iter()
        .filter_map(|l| {
            let (head, value) = l.split_once(':')?;
            let mut parts = head.split(';');
            // "item1.TEL" → "TEL"
            let name = parts.next()?.rsplit('.').next()?.to_ascii_uppercase();
            Some(Line {
                name,
                params: parts.map(str::to_string).collect(),
                value: value.to_string(),
            })
        })
        .collect()
}

fn quoted_printable(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'=' {
            if let Some(byte) = b
                .get(i + 1..i + 3)
                .and_then(|h| std::str::from_utf8(h).ok())
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A value with its transfer encoding undone (structure characters kept).
fn decoded(line: &Line) -> String {
    if line.has("QUOTED-PRINTABLE") {
        quoted_printable(&line.value)
    } else {
        line.value.clone()
    }
}

/// The text of a value: quoted-printable decoded, 3.0 escapes undone.
fn text(line: &Line) -> String {
    let v = decoded(line);
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') | Some('N') => out.push('\n'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out.trim().to_string()
}

/// `Family;Given;Middle;Prefix;Suffix` → "Prefix Given Middle Family Suffix".
fn name_from_n(line: &Line) -> String {
    let v = decoded(line);
    let p: Vec<&str> = v.split(';').map(str::trim).collect();
    let get = |i: usize| p.get(i).copied().unwrap_or("");
    [get(3), get(1), get(2), get(0), get(4)]
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
}

fn phone_kind(line: &Line) -> &'static str {
    if line.has("CELL") {
        "mobile"
    } else if line.has("WORK") {
        "work"
    } else if line.has("HOME") {
        "home"
    } else {
        ""
    }
}

/// Parse one card. `uid` is the file's name without `.vcf` — the phone's own
/// contact id, stable between syncs.
pub fn parse(uid: &str, vcard: &str) -> Contact {
    let mut c = Contact {
        uid: uid.to_string(),
        ..Default::default()
    };
    let mut from_n = String::new();
    for l in logical_lines(vcard) {
        match l.name.as_str() {
            "FN" => c.name = text(&l),
            "N" => from_n = name_from_n(&l),
            "TEL" => {
                // vCard 4 writes a number as a link: TEL;VALUE=uri:tel:+1…
                let number = text(&l);
                let number = number
                    .strip_prefix("tel:")
                    .unwrap_or(&number)
                    .trim()
                    .to_string();
                if !number.is_empty() && !c.phones.iter().any(|p| p.number == number) {
                    c.phones.push(Phone {
                        number,
                        kind: phone_kind(&l).to_string(),
                    });
                }
            }
            "EMAIL" => {
                let e = text(&l);
                if !e.is_empty() && !c.emails.contains(&e) {
                    c.emails.push(e);
                }
            }
            "ORG" => c.org = text(&l).replace(';', " ").trim().to_string(),
            "NOTE" => c.note = text(&l),
            "PHOTO" => c.has_photo = !l.value.trim().is_empty() && !l.has("URI"),
            _ => {}
        }
    }
    if c.name.is_empty() {
        c.name = from_n;
    }
    if c.name.is_empty() {
        // a card with a number and no name is still someone to write to
        c.name = c
            .phones
            .first()
            .map(|p| p.number.clone())
            .or_else(|| c.emails.first().cloned())
            .unwrap_or_default();
    }
    c
}

/// The card's photo as a `data:` URL ("" when it has none).
pub fn photo(vcard: &str) -> String {
    for l in logical_lines(vcard) {
        if l.name != "PHOTO" {
            continue;
        }
        let is_b64 = l.has("BASE64")
            || l.param("ENCODING")
                .is_some_and(|e| e.eq_ignore_ascii_case("b"));
        if !is_b64 {
            continue;
        }
        let compact: String = l.value.chars().filter(|c| !c.is_whitespace()).collect();
        if let Some(bytes) = crate::tools::b64_decode(&compact) {
            return crate::tools::data_url(&bytes);
        }
    }
    String::new()
}

const MAX_CARD: u64 = 4 * 1024 * 1024;

/// A contact id as the daemon writes it: one file name, no way out of the
/// folder.
fn uid_ok(uid: &str) -> bool {
    !uid.is_empty()
        && uid.len() <= 255
        && !uid.contains('/')
        && !uid.contains('\0')
        && uid != "."
        && uid != ".."
}

/// Every contact of a device, by name. A phone that never synced (or denied
/// the contacts permission) has no folder: an empty book, not an error.
pub fn load(dir: &Path) -> Vec<Contact> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<Contact> = entries
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("vcf") {
                return None;
            }
            let uid = p.file_stem()?.to_str()?.to_string();
            // cards are small — one with a photo is tens of kilobytes; a
            // file far beyond that is not a contact worth the memory
            if e.metadata().ok()?.len() > MAX_CARD {
                return None;
            }
            let bytes = std::fs::read(&p).ok()?;
            let c = parse(&uid, &String::from_utf8_lossy(&bytes));
            (!c.name.is_empty()).then_some(c)
        })
        .collect();
    out.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.uid.cmp(&b.uid))
    });
    out
}

pub fn load_photo(dir: &Path, uid: &str) -> String {
    if !uid_ok(uid) {
        return String::new();
    }
    let path = dir.join(format!("{uid}.vcf"));
    match std::fs::metadata(&path) {
        Ok(m) if m.is_file() && m.len() <= MAX_CARD => {}
        _ => return String::new(),
    }
    std::fs::read(path)
        .map(|b| photo(&String::from_utf8_lossy(&b)))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    // what Android's KDE Connect writes (vCard 2.1), with made-up people
    const ANDROID: &str = "BEGIN:VCARD\r\nVERSION:2.1\r\nN:Example;Nino;;;\r\nFN:Nino Example\r\nTEL;CELL;PREF:+1 202 555 0101\r\nTEL;HOME:202 555 0111\r\nEMAIL;HOME:nino@example.org\r\nPHOTO;ENCODING=BASE64;JPEG:/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAIBAQEBAQIB\r\n AQECAgICAgQDAgICAgUEBAMEBgUGBgYFBgYGBwkIBgcJBwYGCAsICQoKCgoKBggLDAsK\r\n\r\nX-KDECONNECT-ID-DEV-abc:42\r\nREV:20261101T101500Z\r\nEND:VCARD\r\n";

    #[test]
    fn android_card() {
        let c = parse("42", ANDROID);
        assert_eq!(c.name, "Nino Example");
        assert_eq!(
            c.phones,
            vec![
                Phone {
                    number: "+1 202 555 0101".into(),
                    kind: "mobile".into()
                },
                Phone {
                    number: "202 555 0111".into(),
                    kind: "home".into()
                },
            ]
        );
        assert_eq!(c.emails, vec!["nino@example.org"]);
        assert!(c.has_photo);
        assert!(photo(ANDROID).starts_with("data:image/jpeg;base64,/9j/"));
    }

    #[test]
    fn quoted_printable_utf8_name_with_a_soft_break() {
        // "ნინო მაგალითი" in UTF-8, quoted-printable, wrapped mid-value
        let card = "BEGIN:VCARD\nVERSION:2.1\nN;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:=E1=83=9B=E1=83=90=E1=83=92=E1=83=90=E1=83=9A=E1=83=98=E1=83=97=E1=83=98;=\n=E1=83=9C=E1=83=98=E1=83=9C=E1=83=9D;;;\nFN;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:=E1=83=9C=E1=83=98=E1=83=9C=E1=83=9D =E1=83=9B=E1=83=90=E1=83=92=E1=83=90=\n=E1=83=9A=E1=83=98=E1=83=97=E1=83=98\nTEL;CELL:555000003\nEND:VCARD\n";
        let c = parse("7", card);
        assert_eq!(c.name, "ნინო მაგალითი");
        assert_eq!(c.phones[0].number, "555000003");
        assert!(!c.has_photo);
    }

    #[test]
    fn name_falls_back_to_n_then_to_the_number() {
        let c = parse("1", "BEGIN:VCARD\nN:Example;Gio;;Dr.;\nTEL:1\nEND:VCARD");
        assert_eq!(c.name, "Dr. Gio Example");
        let c = parse(
            "2",
            "BEGIN:VCARD\nTEL;TYPE=WORK,VOICE:+1 555 0100\nEND:VCARD",
        );
        assert_eq!(c.name, "+1 555 0100");
        assert_eq!(c.phones[0].kind, "work");
        // vCard 4: quoted parameter values, the number as a tel: link
        let c = parse(
            "4",
            "BEGIN:VCARD\nVERSION:4.0\nFN:Ana\nTEL;VALUE=uri;TYPE=\"cell,voice\":tel:+1-555-0101\nEND:VCARD",
        );
        assert_eq!(c.phones[0].number, "+1-555-0101");
        assert_eq!(c.phones[0].kind, "mobile");
    }

    #[test]
    fn vcard3_grouped_lines_and_escapes() {
        let card = "BEGIN:VCARD\nVERSION:3.0\nFN:A\\, B\nitem1.TEL;TYPE=CELL:123\nNOTE:line one\\nline two\nORG:Acme;Lab\nPHOTO;ENCODING=b;TYPE=PNG:iVBORw0KGgo=\nEND:VCARD";
        let c = parse("3", card);
        assert_eq!(c.name, "A, B");
        assert_eq!(c.phones[0].kind, "mobile");
        assert_eq!(c.note, "line one\nline two");
        assert_eq!(c.org, "Acme Lab");
        assert!(photo(card).starts_with("data:image/png;base64,"));
    }

    #[test]
    fn a_contact_id_cannot_leave_the_folder() {
        assert!(uid_ok("1234r5678-ABCD.EF"));
        assert!(!uid_ok("../secret"));
        assert!(!uid_ok(""));
        assert!(!uid_ok(".."));
        assert_eq!(load_photo(Path::new("/nonexistent"), "../x"), "");
    }
}
