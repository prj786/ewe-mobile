//! The phone's apps on this computer — contracts C1, C2 and C4 of
//! `docs/ECOSYSTEM.md`.
//!
//! - **C1**: ewe Mobile for Android announces its launchable apps as a KDE
//!   Connect *command list* (`remotecommands.commands`, UTF-8 JSON in a byte
//!   array). A list is ours when it carries an `"ewe"` key with `v` ≥ 1;
//!   anything else is someone's real command list and is left alone. Keys,
//!   names and icons are strings a phone wrote: every one is checked here.
//! - **C2**: the cache every reader shares —
//!   `$XDG_CACHE_HOME/ewe-mobile/<deviceId>/apps.json` and `icons/<pkg>.png`
//!   — so the dock and the launcher work while the phone is away.
//! - **C4**: one launcher entry per app
//!   (`~/.local/share/applications/ewe-mobile-app-<pkg>.desktop`), hidden
//!   behind the setting "Show the phone's apps in the launcher", plus the
//!   entry for the mirrored screen.
//!
//! Nothing here trusts the phone, and nothing here goes through a shell.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Map, Value as Json};

use crate::bus::Bus;
use crate::kdc::dev_path;
use crate::tools;

const REMOTE: &str = "org.kde.kdeconnect.device.remotecommands";

/// The whole list may be this big (the phone keeps it near 700 KB).
const RAW_MAX: usize = 8 * 1024 * 1024;
/// A decoded icon may be this big.
pub const ICON_MAX: usize = 256 * 1024;
const NAME_MAX: usize = 200;
const APPS_MAX: usize = 5000;
const ICON_PX: std::ops::RangeInclusive<u32> = 16..=512;

/// One launchable app, as the phone described it and this file checked it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    pub package: String,
    pub name: String,
    pub sys: bool,
    /// A square PNG, or None when the phone sent none (or a bad one).
    pub icon: Option<Vec<u8>>,
}

/// One row of `apps.json` (C2).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Cached {
    pub package: String,
    pub name: String,
    pub sys: bool,
    /// Absolute path of its icon file, "" when it has none.
    pub icon: String,
}

// ── C1: the list, checked ───────────────────────────────────────────────────

/// `[A-Za-z0-9._]{1,255}`, no leading, trailing or doubled dot (a package is
/// a file name here and part of a window class).
pub fn package_ok(k: &str) -> bool {
    !k.is_empty()
        && k.len() <= 255
        && k.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
        && !k.starts_with('.')
        && !k.ends_with('.')
        && !k.contains("..")
}

/// One short line: no control or direction-formatting characters, spaces
/// collapsed, at most 200 characters.
pub fn clean_name(v: &str) -> String {
    let dropped = |c: char| {
        c.is_control()
            || matches!(c,
                '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
    };
    let mut out = String::new();
    let mut space = true; // leading blanks are dropped
    for c in v.chars() {
        if dropped(c) {
            continue;
        }
        if c.is_whitespace() {
            if !space {
                out.push(' ');
                space = true;
            }
            continue;
        }
        space = false;
        out.push(c);
        if out.chars().count() >= NAME_MAX {
            break;
        }
    }
    out.trim_end().to_string()
}

/// A real PNG: the magic, an IHDR first, square, a size the shell can draw,
/// and no bigger than ICON_MAX.
pub fn png_ok(data: &[u8]) -> bool {
    if data.len() < 33 || data.len() > ICON_MAX {
        return false;
    }
    if !data.starts_with(b"\x89PNG\r\n\x1a\n") || &data[12..16] != b"IHDR" {
        return false;
    }
    let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
    let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
    w == h && ICON_PX.contains(&w)
}

/// The `commands` property as the bus hands it (`ay` → a JSON array of
/// numbers, or a string if a stand-in sent one) → bytes.
pub fn commands_bytes(v: &Json) -> Option<Vec<u8>> {
    match v {
        Json::Array(a) => a
            .iter()
            .map(|x| x.as_u64().and_then(|n| u8::try_from(n).ok()))
            .collect(),
        Json::String(s) => Some(s.as_bytes().to_vec()),
        _ => None,
    }
}

/// The checked list, or None when this is not ewe Mobile's list (no `"ewe"`
/// key, `v` < 1, not JSON): a phone running the stock app with its own
/// commands must never show up as "apps".
pub fn parse_commands(raw: &[u8]) -> Option<Vec<App>> {
    if raw.is_empty() || raw.len() > RAW_MAX {
        return None;
    }
    let text = std::str::from_utf8(raw).ok()?;
    let root: Map<String, Json> = match serde_json::from_str(text).ok()? {
        Json::Object(m) => m,
        _ => return None,
    };
    let marker = root.get("ewe")?.as_object()?;
    if marker.get("v").and_then(Json::as_i64).unwrap_or(0) < 1 {
        return None;
    }
    let mut out = Vec::new();
    for (key, entry) in &root {
        if key == "ewe" || key.starts_with("ewe:") {
            continue;
        }
        let Some(e) = entry.as_object() else { continue };
        if !package_ok(key) {
            continue;
        }
        // the key is the package; `command` must agree when it is there
        if let Some(c) = e.get("command").and_then(Json::as_str) {
            if !c.is_empty() && c != key {
                continue;
            }
        }
        let name = clean_name(e.get("name").and_then(Json::as_str).unwrap_or(""));
        let name = if name.is_empty() { key.clone() } else { name };
        let icon = e
            .get("icon")
            .and_then(Json::as_str)
            .filter(|s| s.len() <= ICON_MAX * 4 / 3 + 4)
            .and_then(tools::b64_decode)
            .filter(|b| png_ok(b));
        out.push(App {
            package: key.clone(),
            name,
            sys: e.get("sys").and_then(Json::as_bool).unwrap_or(false),
            icon,
        });
        if out.len() >= APPS_MAX {
            break;
        }
    }
    sort_apps(&mut out);
    Some(out)
}

/// By name, case-insensitively (C2's order), then by package so the order is
/// total.
pub fn sort_apps(apps: &mut [App]) {
    apps.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.package.cmp(&b.package))
    });
}

// ── C1 on the bus ───────────────────────────────────────────────────────────

/// The raw `commands` bytes of a device, None when it has no
/// `remotecommands` object at all (the phone does not announce commands).
pub async fn commands_of(bus: &Bus, device: &str) -> Result<Option<Vec<u8>>, String> {
    let path = format!("{}/remotecommands", dev_path(device)?);
    match bus.get_all(&path, REMOTE).await {
        Ok(props) => Ok(props.get("commands").and_then(commands_bytes)),
        // the daemon says "no such object" for a plugin that is not loaded
        Err(e)
            if e.contains("UnknownObject")
                || e.contains("No such object")
                || e.contains("doesn't exist") =>
        {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

/// The phone's apps, live: Some(list) when the device is ewe Mobile and its
/// list arrived, None otherwise.
pub async fn apps_live(bus: &Bus, device: &str) -> Result<Option<Vec<App>>, String> {
    Ok(commands_of(bus, device)
        .await?
        .and_then(|raw| parse_commands(&raw)))
}

/// Ask the phone for something by key: a package starts that app on its
/// screen, `ewe:refresh` sends the list again.
pub async fn trigger(bus: &Bus, device: &str, key: &str) -> Result<(), String> {
    if !(key == "ewe:refresh" || package_ok(key)) {
        return Err("unknown-app".into());
    }
    let path = format!("{}/remotecommands", dev_path(device)?);
    bus.call(&path, REMOTE, "triggerCommand", &(key,))
        .await
        .map(|_| ())
}

// ── C2: the cache ───────────────────────────────────────────────────────────

pub fn cache_base() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| tools::home().join(".cache"))
        .join("ewe-mobile")
}

fn id_ok(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

pub fn device_dir(base: &Path, dev_id: &str) -> Option<PathBuf> {
    id_ok(dev_id).then(|| base.join(dev_id))
}

/// Whole file or nothing: a temp file beside it, then rename.
fn atomic_write(path: &Path, data: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path)
}

/// Write `apps.json` and the icons; the rows as written. Icons are rewritten
/// only when their bytes changed; a package that left the list loses its
/// icon file.
pub fn write_cache(
    base: &Path,
    dev_id: &str,
    dev_name: &str,
    apps: &[App],
) -> io::Result<Vec<Cached>> {
    let dir = device_dir(base, dev_id).ok_or_else(|| io::Error::other("bad device id"))?;
    let icons = dir.join("icons");
    fs::create_dir_all(&icons)?;
    let mut rows = Vec::with_capacity(apps.len());
    let mut keep = BTreeSet::new();
    for a in apps {
        let mut icon_path = String::new();
        if let Some(png) = &a.icon {
            let p = icons.join(format!("{}.png", a.package));
            let same = fs::read(&p).map(|old| old == *png).unwrap_or(false);
            if !same {
                atomic_write(&p, png)?;
            }
            keep.insert(p.file_name().unwrap().to_os_string());
            icon_path = p.display().to_string();
        }
        rows.push(Cached {
            package: a.package.clone(),
            name: a.name.clone(),
            sys: a.sys,
            icon: icon_path,
        });
    }
    if let Ok(rd) = fs::read_dir(&icons) {
        for e in rd.flatten() {
            if !keep.contains(&e.file_name()) {
                let _ = fs::remove_file(e.path());
            }
        }
    }
    let doc = json!({
        "v": 1,
        "device": dev_id,
        "deviceName": dev_name,
        "updated": now_ms(),
        "apps": rows,
    });
    atomic_write(&dir.join("apps.json"), doc.to_string().as_bytes())?;
    Ok(rows)
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// `apps.json` back, checked the same way (another writer may have been
/// careless): the rows, the phone's name and when it was written.
pub fn read_cache(base: &Path, dev_id: &str) -> Option<(Vec<Cached>, String, u64)> {
    let dir = device_dir(base, dev_id)?;
    let text = fs::read_to_string(dir.join("apps.json")).ok()?;
    let doc: Json = serde_json::from_str(&text).ok()?;
    if doc["v"].as_i64() != Some(1) {
        return None;
    }
    let icons = dir.join("icons");
    let mut rows = Vec::new();
    for a in doc["apps"].as_array()? {
        let Some(package) = a["package"].as_str().filter(|p| package_ok(p)) else {
            continue;
        };
        let name = clean_name(a["name"].as_str().unwrap_or(""));
        // only an icon inside this device's own folder
        let icon = a["icon"]
            .as_str()
            .map(Path::new)
            .filter(|p| p.parent() == Some(icons.as_path()) && p.is_file())
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        rows.push(Cached {
            package: package.to_string(),
            name: if name.is_empty() {
                package.to_string()
            } else {
                name
            },
            sys: a["sys"].as_bool().unwrap_or(false),
            icon,
        });
    }
    let name = clean_name(doc["deviceName"].as_str().unwrap_or(""));
    Some((rows, name, doc["updated"].as_u64().unwrap_or(0)))
}

pub fn remove_cache(base: &Path, dev_id: &str) {
    if let Some(dir) = device_dir(base, dev_id) {
        let _ = fs::remove_dir_all(dir);
    }
}

/// Every device that has a cache folder.
pub fn cached_devices(base: &Path) -> Vec<String> {
    fs::read_dir(base)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|id| id_ok(id))
                .collect()
        })
        .unwrap_or_default()
}

/// A cached row as the window shows it: the icon file as a `data:` URL.
pub fn row_json(r: &Cached) -> Json {
    let icon = if r.icon.is_empty() {
        String::new()
    } else {
        tools::picture_file(Path::new(&r.icon), ICON_MAX as u64)
    };
    json!({ "package": r.package, "name": r.name, "sys": r.sys, "icon": icon })
}

// ── C4: launcher entries ────────────────────────────────────────────────────

const MARK: &str = "X-Ewe-Mobile-App";

pub fn entries_dir() -> PathBuf {
    tools::data_home().join("applications")
}

pub fn app_class(package: &str) -> String {
    format!("ewe-mobile-app-{package}")
}

/// A desktop-entry string value: no newlines (names are clean already),
/// backslashes doubled.
fn de_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
}

/// The text of one app's entry. `X-Eep-Skip` is this app's mark for eep,
/// whose card rules give every launcher app a `~/Applications` card: these
/// must not get one (see the report in README).
pub fn app_entry_text(dev_id: &str, dev_name: &str, a: &Cached, show: bool) -> String {
    let icon = if a.icon.is_empty() {
        "io.github.prj786.ewe-mobile"
    } else {
        a.icon.as_str()
    };
    format!(
        "[Desktop Entry]\nType=Application\nName={}\nGenericName=On {}\nComment=Opens on this PC from your phone\n\
         Icon={}\nExec=ewe-mobile --open-app {}\nTerminal=false\nStartupWMClass={}\n\
         Categories=X-EweMobilePhone;\n{MARK}={}\nX-Eep-Skip=true\nNoDisplay={}\n",
        de_escape(&a.name),
        de_escape(dev_name),
        de_escape(icon),
        a.package,
        app_class(&a.package),
        dev_id,
        if show { "false" } else { "true" }
    )
}

pub fn screen_entry_text(dev_id: &str, dev_name: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName={} screen\nGenericName=Phone screen\n\
         Comment=Your phone’s screen in a window\nIcon=io.github.prj786.ewe-mobile\n\
         Exec=ewe-mobile --screen\nTerminal=false\nStartupWMClass=ewe-mobile-screen\n\
         Categories=X-EweMobilePhone;\n{MARK}={}\nX-Eep-Skip=true\nNoDisplay=true\n",
        de_escape(dev_name),
        dev_id
    )
}

fn marked_for(text: &str) -> Option<&str> {
    text.lines()
        .find_map(|l| l.strip_prefix(&format!("{MARK}=")))
        .map(str::trim)
}

/// Only files that are ours — `ewe-mobile-*.desktop` carrying our mark —
/// are ever written over or deleted.
fn our_entries(dir: &Path, dev_id: Option<&str>) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    name.starts_with("ewe-mobile-") && name.ends_with(".desktop")
                })
                .filter(|p| {
                    fs::read_to_string(p)
                        .ok()
                        .and_then(|t| marked_for(&t).map(|m| dev_id.is_none_or(|d| d == m)))
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Write the entries of this phone's apps (and the screen's), delete the
/// ones of apps that left. A file is rewritten only when its text changed,
/// so the launcher is not poked for nothing.
pub fn write_entries(
    dir: &Path,
    dev_id: &str,
    dev_name: &str,
    apps: &[Cached],
    show: bool,
) -> io::Result<usize> {
    if !id_ok(dev_id) {
        return Err(io::Error::other("bad device id"));
    }
    fs::create_dir_all(dir)?;
    let mut wanted = BTreeSet::new();
    let mut written = 0;
    let mut put = |file: PathBuf, text: String| -> io::Result<()> {
        let same = fs::read_to_string(&file)
            .map(|old| old == text)
            .unwrap_or(false);
        if !same {
            atomic_write(&file, text.as_bytes())?;
            written += 1;
        }
        wanted.insert(file);
        Ok(())
    };
    for a in apps {
        put(
            dir.join(format!("{}.desktop", app_class(&a.package))),
            app_entry_text(dev_id, dev_name, a, show),
        )?;
    }
    put(
        dir.join("ewe-mobile-screen.desktop"),
        screen_entry_text(dev_id, dev_name),
    )?;
    for stale in our_entries(dir, Some(dev_id)) {
        if !wanted.contains(&stale) {
            let _ = fs::remove_file(stale);
        }
    }
    Ok(written)
}

/// Every entry of this phone (unpaired, removed).
pub fn remove_entries(dir: &Path, dev_id: &str) {
    for p in our_entries(dir, Some(dev_id)) {
        let _ = fs::remove_file(p);
    }
}

// ── the setting ─────────────────────────────────────────────────────────────

const SETTING: &str = "mobile.launcher_apps";

/// Where ewe keeps settings: `ewe-conf` (rule 1 — the only writer of
/// ewe.conf). Without it (ewe not deployed) the default stands.
async fn ewe_conf(args: &[&str]) -> Option<String> {
    let path = tools::ewe_tool("ewe-conf")?;
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        tokio::process::Command::new("python3")
            .arg(&path)
            .args(args)
            .env("LANG", "C")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// "Show the phone's apps in the launcher" — default on.
pub async fn launcher_setting() -> bool {
    !matches!(
        ewe_conf(&["get", SETTING]).await.as_deref(),
        Some("false") | Some("0")
    )
}

pub async fn set_launcher_setting(on: bool) -> Result<(), String> {
    if tools::ewe_tool("ewe-conf").is_none() {
        return Err(
            "ewe isn’t set up on this computer, so the setting has nowhere to live.".into(),
        );
    }
    let v = if on { "true" } else { "false" };
    let out = ewe_conf(&["set", "--no-hooks", SETTING, v]).await;
    match out {
        Some(o) if !o.contains("\"error\"") => Ok(()),
        _ => Err("ewe-conf refused the setting.".into()),
    }
}

// ── the whole picture, for the window and the CLI ───────────────────────────

/// Read the phone's list if it is in reach and ours, write C2 and C4, and
/// say where the list came from: `phone`, `cache` or `none`.
pub async fn sync(
    bus: &Bus,
    dev_id: &str,
    dev_name: &str,
    reachable: bool,
) -> (String, Vec<Cached>, u64) {
    let base = cache_base();
    if reachable {
        if let Ok(Some(list)) = apps_live(bus, dev_id).await {
            match write_cache(&base, dev_id, dev_name, &list) {
                Ok(rows) => {
                    let show = launcher_setting().await;
                    if let Err(e) = write_entries(&entries_dir(), dev_id, dev_name, &rows, show) {
                        eprintln!("ewe-mobile: launcher entries: {e}");
                    }
                    return ("phone".into(), rows, now_ms());
                }
                Err(e) => eprintln!("ewe-mobile: apps cache: {e}"),
            }
        }
    }
    match read_cache(&base, dev_id) {
        Some((rows, _, updated)) => ("cache".into(), rows, updated),
        None => ("none".into(), Vec::new(), 0),
    }
}

/// Drop everything kept for phones that are no longer paired.
pub fn prune(paired: &[String]) {
    let base = cache_base();
    for id in cached_devices(&base) {
        if !paired.contains(&id) {
            remove_cache(&base, &id);
            remove_entries(&entries_dir(), &id);
        }
    }
    // entries whose cache folder is gone (a cache cleaned by hand)
    for p in our_entries(&entries_dir(), None) {
        if let Some(id) = fs::read_to_string(&p)
            .ok()
            .and_then(|t| marked_for(&t).map(String::from))
        {
            if !paired.contains(&id) {
                let _ = fs::remove_file(p);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    /// A 1×1 PNG grown to `px` by rewriting IHDR (the decoder is never run
    /// here, only the header is checked).
    pub fn png(px: u32) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        v.extend_from_slice(&px.to_be_bytes());
        v.extend_from_slice(&px.to_be_bytes());
        v.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]); // depth, type, …, crc
        v.extend_from_slice(b"\0\0\0\0IEND\xaeB`\x82");
        v
    }
    fn b64(b: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(b)
    }
    fn list(extra: &str) -> Vec<u8> {
        format!(
            r#"{{"ewe":{{"name":"ewe Mobile","command":"","v":1,"count":2}},
                "com.whatsapp":{{"name":"WhatsApp","command":"com.whatsapp","icon":"{}","sys":false}},
                "com.android.settings":{{"name":"Settings","command":"com.android.settings","sys":true}}{extra}}}"#,
            b64(&png(96))
        )
        .into_bytes()
    }

    #[test]
    fn our_list_is_recognised_and_sorted() {
        let apps = parse_commands(&list("")).unwrap();
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].name, "Settings"); // s < w
        assert!(apps[0].sys && apps[0].icon.is_none());
        assert_eq!(apps[1].package, "com.whatsapp");
        assert!(png_ok(apps[1].icon.as_ref().unwrap()));
    }

    #[test]
    fn someone_elses_commands_are_not_apps() {
        assert!(
            parse_commands(br#"{"a1":{"name":"Lock","command":"loginctl lock-session"}}"#)
                .is_none()
        );
        assert!(parse_commands(br#"{"ewe":{"v":0},"com.x":{"name":"X"}}"#).is_none());
        assert!(parse_commands(b"not json").is_none());
        assert!(parse_commands(b"[]").is_none());
        assert!(parse_commands(b"").is_none());
        // the stock app's empty list
        assert!(parse_commands(b"{}").is_none());
    }

    #[test]
    fn hostile_keys_names_and_icons() {
        let apps = parse_commands(&list(
            r#",
            "../../etc":{"name":"Escape"},
            "has space":{"name":"Nope"},
            "a..b":{"name":"Dots"},
            ".lead":{"name":"Lead"},
            "ewe:refresh":{"name":"Reserved"},
            "com.liar":{"name":"Liar","command":"rm -rf /"},
            "com.noname":{"command":"com.noname"},
            "com.sneaky":{"name":"Hi\u202Ethere\n\t  friend","icon":"bm90IGEgcG5n"},
            "com.webp":{"name":"WebP","icon":"UklGRiQAAABXRUJQVlA4"},
            "com.string":"just a string""#,
        ))
        .unwrap();
        let names: Vec<_> = apps.iter().map(|a| a.package.as_str()).collect();
        assert_eq!(
            names,
            [
                "com.noname",
                "com.sneaky", // "Hithere friend"
                "com.android.settings",
                "com.webp",
                "com.whatsapp"
            ]
        );
        let sneaky = apps.iter().find(|a| a.package == "com.sneaky").unwrap();
        assert_eq!(sneaky.name, "Hithere friend");
        assert!(sneaky.icon.is_none(), "a non-PNG icon is dropped");
        assert!(apps
            .iter()
            .find(|a| a.package == "com.webp")
            .unwrap()
            .icon
            .is_none());
        assert_eq!(
            apps.iter()
                .find(|a| a.package == "com.noname")
                .unwrap()
                .name,
            "com.noname"
        );
    }

    #[test]
    fn long_names_are_cut_and_icons_must_be_square_pngs() {
        let long = "x".repeat(500);
        assert_eq!(clean_name(&long).chars().count(), 200);
        assert_eq!(clean_name("  a \u{0}b  "), "a b");
        assert!(png_ok(&png(96)));
        assert!(png_ok(&png(16)) && png_ok(&png(512)));
        assert!(!png_ok(&png(8)) && !png_ok(&png(1024)));
        let mut rect = png(96);
        rect[20..24].copy_from_slice(&64u32.to_be_bytes());
        assert!(!png_ok(&rect));
        let mut big = png(96);
        big.extend(std::iter::repeat_n(0u8, ICON_MAX));
        assert!(!png_ok(&big));
        assert!(!png_ok(b"\xff\xd8\xff\xe0 jpeg"));
    }

    #[test]
    fn the_bus_bytes_are_an_array_of_numbers() {
        assert_eq!(
            commands_bytes(&json!([123, 125])).as_deref(),
            Some(&b"{}"[..])
        );
        assert_eq!(commands_bytes(&json!("{}")).as_deref(), Some(&b"{}"[..]));
        assert_eq!(commands_bytes(&json!([300])), None);
        assert_eq!(commands_bytes(&json!(5)), None);
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ewe-mobile-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_cache_is_written_pruned_and_read_back() {
        let base = tmp("cache");
        let apps = parse_commands(&list("")).unwrap();
        let rows = write_cache(&base, "dev1", "Pixel", &apps).unwrap();
        assert_eq!(rows.len(), 2);
        let dir = base.join("dev1");
        assert!(dir.join("apps.json").is_file());
        assert!(dir.join("icons/com.whatsapp.png").is_file());
        assert_eq!(
            rows[1].icon,
            dir.join("icons/com.whatsapp.png").display().to_string()
        );
        assert_eq!(rows[0].icon, "");
        let doc: Json =
            serde_json::from_str(&fs::read_to_string(dir.join("apps.json")).unwrap()).unwrap();
        assert_eq!(doc["v"], 1);
        assert_eq!(doc["device"], "dev1");
        assert_eq!(doc["deviceName"], "Pixel");
        assert!(doc["updated"].as_u64().unwrap() > 1_700_000_000_000);
        assert_eq!(doc["apps"][1]["package"], "com.whatsapp");

        // an icon rewritten only when its bytes change
        let m1 = fs::metadata(dir.join("icons/com.whatsapp.png"))
            .unwrap()
            .modified()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_cache(&base, "dev1", "Pixel", &apps).unwrap();
        assert_eq!(
            fs::metadata(dir.join("icons/com.whatsapp.png"))
                .unwrap()
                .modified()
                .unwrap(),
            m1
        );

        // a package that left loses its icon
        let only_settings: Vec<App> = apps.iter().filter(|a| a.sys).cloned().collect();
        write_cache(&base, "dev1", "Pixel", &only_settings).unwrap();
        assert!(!dir.join("icons/com.whatsapp.png").exists());

        let (back, name, _) = read_cache(&base, "dev1").unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(name, "Pixel");
        assert!(read_cache(&base, "nobody").is_none());
        assert!(write_cache(&base, "../x", "P", &apps).is_err());
        assert_eq!(cached_devices(&base), vec!["dev1".to_string()]);
        remove_cache(&base, "dev1");
        assert!(!dir.exists());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn a_careless_cache_is_read_defensively() {
        let base = tmp("careless");
        let dir = base.join("dev2");
        fs::create_dir_all(dir.join("icons")).unwrap();
        fs::write(
            dir.join("apps.json"),
            format!(
                r#"{{"v":1,"device":"dev2","deviceName":"P","updated":1,"apps":[
                  {{"package":"com.ok","name":"OK","sys":false,"icon":"/etc/passwd"}},
                  {{"package":"bad key!","name":"Bad"}},
                  {{"package":"com.noicon","name":"No icon","icon":"{}/icons/missing.png"}}]}}"#,
                dir.display()
            ),
        )
        .unwrap();
        let (rows, _, _) = read_cache(&base, "dev2").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0].icon, "",
            "an icon outside the device's folder is not used"
        );
        assert_eq!(rows[1].icon, "", "a missing icon file is not used");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn launcher_entries_follow_the_list_and_the_setting() {
        let dir = tmp("entries");
        let rows = vec![
            Cached {
                package: "com.whatsapp".into(),
                name: "WhatsApp".into(),
                sys: false,
                icon: "/x/icons/com.whatsapp.png".into(),
            },
            Cached {
                package: "com.a".into(),
                name: "A \\ B".into(),
                sys: true,
                icon: "".into(),
            },
        ];
        // someone else's file with our prefix must survive
        fs::write(
            dir.join("ewe-mobile-app-com.other.desktop"),
            "[Desktop Entry]\nName=Other\n",
        )
        .unwrap();
        let n = write_entries(&dir, "dev1", "Pixel 10", &rows, true).unwrap();
        assert_eq!(n, 3);
        let wa = fs::read_to_string(dir.join("ewe-mobile-app-com.whatsapp.desktop")).unwrap();
        assert!(wa.contains("Name=WhatsApp\n"));
        assert!(wa.contains("GenericName=On Pixel 10\n"));
        assert!(wa.contains("Icon=/x/icons/com.whatsapp.png\n"));
        assert!(wa.contains("Exec=ewe-mobile --open-app com.whatsapp\n"));
        assert!(wa.contains("StartupWMClass=ewe-mobile-app-com.whatsapp\n"));
        assert!(wa.contains("Categories=X-EweMobilePhone;\n"));
        assert!(wa.contains("X-Ewe-Mobile-App=dev1\n"));
        assert!(wa.contains("NoDisplay=false\n"));
        let a = fs::read_to_string(dir.join("ewe-mobile-app-com.a.desktop")).unwrap();
        assert!(a.contains("Name=A \\\\ B\n"));
        assert!(a.contains("Icon=io.github.prj786.ewe-mobile\n"));
        let screen = fs::read_to_string(dir.join("ewe-mobile-screen.desktop")).unwrap();
        assert!(
            screen.contains("Exec=ewe-mobile --screen\n") && screen.contains("NoDisplay=true\n")
        );

        // nothing changed: nothing written
        assert_eq!(
            write_entries(&dir, "dev1", "Pixel 10", &rows, true).unwrap(),
            0
        );
        // the setting off: every app entry hidden, the file kept
        write_entries(&dir, "dev1", "Pixel 10", &rows, false).unwrap();
        assert!(fs::read_to_string(dir.join("ewe-mobile-app-com.a.desktop"))
            .unwrap()
            .contains("NoDisplay=true\n"));
        // an app that left
        write_entries(&dir, "dev1", "Pixel 10", &rows[..1], true).unwrap();
        assert!(!dir.join("ewe-mobile-app-com.a.desktop").exists());
        assert!(dir.join("ewe-mobile-app-com.other.desktop").exists());
        // unpaired
        remove_entries(&dir, "dev1");
        assert!(!dir.join("ewe-mobile-app-com.whatsapp.desktop").exists());
        assert!(!dir.join("ewe-mobile-screen.desktop").exists());
        assert!(dir.join("ewe-mobile-app-com.other.desktop").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn packages_and_classes() {
        assert!(package_ok("com.whatsapp"));
        assert!(package_ok("a_b.C9"));
        assert!(!package_ok(""));
        assert!(!package_ok("com/whatsapp"));
        assert!(!package_ok(&"a".repeat(256)));
        assert!(!package_ok("-x"));
        assert_eq!(app_class("com.whatsapp"), "ewe-mobile-app-com.whatsapp");
    }
}
