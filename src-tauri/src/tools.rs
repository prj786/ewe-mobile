//! Small things the rest leans on: where files live, the ewe tools (the
//! theme generator), processes started and reaped, pictures as data: URLs.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use base64::Engine;
use serde_json::{json, Value};
use tokio::process::Command;

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(".local/share"))
}

/// Is `bin` on PATH?
pub fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
}

/// Fire-and-forget a process, reaped off-thread (a child nobody waits for
/// stays a zombie for the life of this app).
pub fn spawn_reaped(bin: &str, args: &[&str]) -> bool {
    let child = std::process::Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    match child {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            true
        }
        Err(_) => false,
    }
}

/// `$EWE_REPO/bin/<name>` (a developer checkout), the deployed payload
/// (`~/.local/share/ewe/bin`), then `/usr/bin` — the way every ewe app finds
/// the ewe tools.
pub fn ewe_tool(name: &str) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(repo) = std::env::var_os("EWE_REPO") {
        candidates.push(PathBuf::from(repo).join("bin").join(name));
    }
    let h = home();
    candidates.push(h.join(".local/share/ewe/bin").join(name));
    candidates.push(h.join(".config/quickshell/../../bin").join(name));
    candidates.push(PathBuf::from("/usr/bin").join(name));
    candidates.into_iter().find(|p| p.is_file())
}

/// The DE's live look (the whole user-theme.json): the front end keys its
/// theme refresh on it, so a scheme or accent picked in ewe Settings reaches
/// an open window.
#[tauri::command]
pub async fn de_prefs() -> Result<Value, String> {
    let p = home().join(".config/quickshell/user-theme.json");
    let Ok(text) = std::fs::read_to_string(&p) else {
        return Ok(Value::Null);
    };
    Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
}

/// The generated tokens for the active look, straight from `ewe-theme show`
/// — the generator is reused, never reimplemented, so the derivation exists
/// in one place.
#[tauri::command]
pub async fn theme_tokens() -> Result<Value, String> {
    let Some(path) = ewe_tool("ewe-theme") else {
        return Err("ewe-theme is not installed".into());
    };
    let out = Command::new("python3")
        .arg(&path)
        .args(["show", "ewe"])
        .env("LANG", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .map_err(|e| format!("ewe-theme: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    Ok(stdout
        .lines()
        .rev()
        .find_map(|l| serde_json::from_str::<Value>(l).ok())
        .unwrap_or(Value::Null))
}

/// What kind of picture these bytes are, by their first bytes.
pub fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF8") {
        Some("image/gif")
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// Bytes → a `data:` URL an <img> can show; "" when they are no picture.
pub fn data_url(bytes: &[u8]) -> String {
    match image_mime(bytes) {
        Some(mime) => format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ),
        None => String::new(),
    }
}

/// Base64 as phones write it: padding or none, stray trailing bits.
pub fn b64_decode(text: &str) -> Option<Vec<u8>> {
    use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
    const LENIENT: GeneralPurpose = GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        GeneralPurposeConfig::new()
            .with_decode_padding_mode(DecodePaddingMode::Indifferent)
            .with_decode_allow_trailing_bits(true),
    );
    LENIENT.decode(text).ok()
}

/// Was this file written in the last quarter of an hour? What the daemon
/// calls "received" is partly the phone's word (a shared link arrives as the
/// link itself, and a link may say `file:///…`): only a file that has just
/// landed is one the phone can have sent.
pub fn just_written(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    meta.is_file()
        && meta
            .modified()
            .ok()
            .and_then(|m| m.elapsed().ok())
            .is_some_and(|age| age < std::time::Duration::from_secs(15 * 60))
}

/// A small picture file as a `data:` URL ("" when missing, too big or not a
/// picture). The daemon keeps notification icons and album art as files in
/// its own cache; reading them here keeps the window's CSP at `data:` only,
/// with no file access of its own.
pub fn picture_file(path: &Path, max_bytes: u64) -> String {
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() && m.len() <= max_bytes => {}
        _ => return String::new(),
    }
    std::fs::read(path)
        .map(|b| data_url(&b))
        .unwrap_or_default()
}

/// `file:///a%20b` from an absolute path (what KDE Connect's share takes).
pub fn file_url(path: &Path) -> String {
    let mut out = String::from("file://");
    for b in path.as_os_str().as_encoded_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(*b as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// The path of a `file://` URL (percent-decoded), None for anything else.
pub fn path_of_url(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if let Some(h) = bytes
                .get(i + 1..i + 3)
                .and_then(|h| std::str::from_utf8(h).ok())
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                out.push(h);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    let p = PathBuf::from(String::from_utf8_lossy(&out).into_owned());
    p.is_absolute().then_some(p)
}

/// `{"ok":true,"name":…}` — what `--version` prints (rule 5: JSON, exit 0).
pub fn version_json() -> Value {
    json!({
        "ok": true,
        "name": "ewe-mobile",
        "product": "ewe Mobile",
        "version": env!("CARGO_PKG_VERSION"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_urls_round_trip() {
        let p = Path::new("/home/you/Pictures/ჩემი ფოტო #1.jpg");
        let url = file_url(p);
        assert!(url.starts_with("file:///home/you/Pictures/"));
        assert!(!url.contains(' ') && !url.contains('#'));
        assert_eq!(path_of_url(&url).as_deref(), Some(p));
        assert_eq!(path_of_url("https://example.org/x"), None);
        assert_eq!(path_of_url("file://relative"), None);
    }

    #[test]
    fn base64_with_or_without_padding() {
        assert_eq!(b64_decode("aGk=").as_deref(), Some(&b"hi"[..]));
        assert_eq!(b64_decode("aGk").as_deref(), Some(&b"hi"[..]));
        assert_eq!(b64_decode("aGl").as_deref(), Some(&b"hi"[..])); // trailing bits
        assert_eq!(b64_decode("not base64!"), None);
    }

    #[test]
    fn only_a_fresh_file_counts_as_received() {
        let dir = std::env::temp_dir().join(format!("ewe-mobile-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fresh = dir.join("just-landed.jpg");
        std::fs::write(&fresh, b"x").unwrap();
        assert!(just_written(&fresh));
        assert!(!just_written(&dir)); // a folder is not a file
        assert!(!just_written(&dir.join("never-existed")));
        // a file that has been here for years (this binary's own system)
        assert!(!just_written(Path::new("/etc/hostname")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn pictures_are_recognised_by_content() {
        assert_eq!(
            image_mime(&[0x89, b'P', b'N', b'G', 0, 0]),
            Some("image/png")
        );
        assert_eq!(image_mime(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(image_mime(b"<svg"), None);
        assert_eq!(data_url(b"not a picture"), "");
        assert!(data_url(&[0xFF, 0xD8, 0xFF, 0xE0]).starts_with("data:image/jpeg;base64,"));
    }
}
