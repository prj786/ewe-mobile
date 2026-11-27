//! The debugging link to the phone (RFC-012 §3.4, A2–A4): `adb` over Wi-Fi
//! and `scrcpy` on top of it — the phone's screen in a window, one phone app
//! per window (C3, C4 in `docs/ECOSYSTEM.md`).
//!
//! What this file never does: keep a key (adb's stays adb's, in
//! `~/.android`), pair without the six digits the person read off the phone,
//! run anything through a shell, or wait for good — every call has a limit.
//! Both tools are optional: when they are missing, the page offers them
//! through Komble (`komble --search=…`, the house's way).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value as Json};
use tokio::process::Command;

use crate::tools;

/// adb answers from its server in milliseconds; a network call (pair,
/// connect) can take a few seconds. `scrcpy --list-apps` took 12.5 s on the
/// owner's phone.
const QUICK: Duration = Duration::from_secs(10);
const NETWORK: Duration = Duration::from_secs(25);
const LIST_APPS: Duration = Duration::from_secs(60);

pub const SCREEN_CLASS: &str = "ewe-mobile-screen";

// ── the tools ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct Tools {
    pub adb: Option<PathBuf>,
    pub scrcpy: Option<PathBuf>,
}

fn find(bin: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join(bin))
            .find(|p| p.is_file())
    })
}

pub fn tools() -> Tools {
    Tools {
        adb: find("adb"),
        scrcpy: find("scrcpy"),
    }
}

pub fn tools_json(t: &Tools) -> Json {
    json!({
        "adb": t.adb.is_some(),
        "scrcpy": t.scrcpy.is_some(),
        "packages": ["android-tools", "scrcpy"],
    })
}

/// Hand the missing package to Komble (what every ewe app does for a
/// package it wants). False: Komble is not installed either.
pub fn offer_tools(t: &Tools) -> bool {
    let want = if t.adb.is_none() {
        "android-tools"
    } else {
        "scrcpy"
    };
    Path::new("/usr/bin/komble").is_file()
        && tools::spawn_reaped("/usr/bin/komble", &[&format!("--search={want}")])
}

// ── running adb ─────────────────────────────────────────────────────────────

pub struct Out {
    pub stdout: String,
    pub stderr: String,
}

/// Run adb with these arguments and a time limit; a run past the limit is
/// killed and reported as such.
pub async fn adb(args: &[&str], limit: Duration) -> Result<Out, String> {
    let Some(bin) = tools().adb else {
        return Err("no-adb-tools".into());
    };
    let child = Command::new(&bin)
        .args(args)
        .env("LANG", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(limit, child).await {
        Ok(Ok(o)) => Ok(Out {
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
        }),
        Ok(Err(e)) => Err(format!("adb didn’t start: {e}")),
        Err(_) => Err("adb didn’t answer in time.".into()),
    }
}

// ── what adb says, parsed ───────────────────────────────────────────────────

/// A phone found by mDNS: `_adb-tls-connect` (ready to connect once paired)
/// or `_adb-tls-pairing` (its pairing dialog is open right now).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Service {
    pub name: String,
    /// `connect` | `pairing`
    pub kind: String,
    pub host: String,
    pub port: u16,
}

/// `adb mdns services`:
/// ```text
/// List of discovered mdns services
/// adb-0A1B2C3D4E5F6G-AbCdEf    _adb-tls-connect._tcp    192.168.1.5:39855
/// ```
pub fn parse_services(text: &str) -> Vec<Service> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut parts = line.split(['\t', ' ']).filter(|s| !s.is_empty());
        let (Some(name), Some(kind), Some(addr)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let kind = if kind.starts_with("_adb-tls-connect") {
            "connect"
        } else if kind.starts_with("_adb-tls-pairing") {
            "pairing"
        } else {
            continue;
        };
        let Some((host, port)) = split_addr(addr) else {
            continue;
        };
        if !name_ok(name) {
            continue;
        }
        out.push(Service {
            name: name.to_string(),
            kind: kind.into(),
            host,
            port,
        });
    }
    out
}

/// `host:port` or `[v6]:port`.
fn split_addr(addr: &str) -> Option<(String, u16)> {
    let (h, p) = addr.rsplit_once(':')?;
    let port: u16 = p.parse().ok()?;
    let host = h.trim_matches(['[', ']']);
    host_ok(host).then(|| (host.to_string(), port))
}

/// A host adb will take and nothing else can hide in: an IPv4/IPv6 literal
/// or a hostname (letters, digits, `.`, `-`, `%` for a v6 scope).
pub fn host_ok(h: &str) -> bool {
    !h.is_empty()
        && h.len() <= 253
        && h.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':' | b'%' | b'_'))
        && !h.starts_with('-')
}

fn name_ok(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 128
        && n.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// One line of `adb devices -l`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Device {
    pub serial: String,
    /// `device` | `offline` | `unauthorized` | …
    pub state: String,
    pub model: String,
    pub product: String,
}

/// ```text
/// List of devices attached
/// adb-0A1B2C3D4E5F6G-AbCdEf._adb-tls-connect._tcp device product:komodo model:Pixel_9_Pro_XL device:komodo transport_id:3
/// ```
pub fn parse_devices(text: &str) -> Vec<Device> {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with("List of devices") || line.starts_with('*') {
            continue; // the heading; "* daemon started successfully"
        }
        let mut parts = line.split_whitespace();
        let (Some(serial), Some(state)) = (parts.next(), parts.next()) else {
            continue;
        };
        let mut d = Device {
            serial: serial.into(),
            state: state.into(),
            model: String::new(),
            product: String::new(),
        };
        for kv in parts {
            if let Some(m) = kv.strip_prefix("model:") {
                d.model = m.into();
            } else if let Some(p) = kv.strip_prefix("product:") {
                d.product = p.into();
            }
        }
        out.push(d);
    }
    out
}

/// `adb pair` → the guid it paired with, or adb's words for what went wrong.
pub fn parse_pair(out: &Out) -> Result<String, String> {
    let text = format!("{}\n{}", out.stdout, out.stderr);
    if let Some(l) = text.lines().find(|l| l.contains("Successfully paired")) {
        let guid = l
            .split("guid=")
            .nth(1)
            .map(|g| g.trim_end_matches(']').trim().to_string());
        return Ok(guid.unwrap_or_default());
    }
    let msg = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("adb said nothing");
    Err(
        if msg.contains("protocol fault") || msg.contains("failed") && msg.contains("pair") {
            "The phone refused the code. Read the six digits again — each attempt gets a new code."
                .into()
        } else {
            msg.to_string()
        },
    )
}

/// `adb connect` → ok when connected (or already was).
pub fn parse_connect(out: &Out) -> Result<(), String> {
    let text = format!("{}\n{}", out.stdout, out.stderr);
    if text.contains("connected to")
        && !text.contains("cannot connect")
        && !text.contains("failed to connect")
    {
        return Ok(());
    }
    Err(text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("adb said nothing")
        .to_string())
}

/// Letters and digits only, lower-case: how "Pixel 9 Pro XL" and
/// `Pixel_9_Pro_XL` become the same word.
fn squash(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

/// Which adb device is this KDE Connect phone? By model against the
/// phone's name (people rarely rename a Pixel); failing that, the only one
/// there is. None: several and none of them says so.
pub fn match_device<'a>(devices: &'a [Device], phone_name: &str) -> Option<&'a Device> {
    let ready: Vec<&Device> = devices.iter().filter(|d| d.state == "device").collect();
    let name = squash(phone_name);
    if !name.is_empty() {
        if let Some(d) = ready.iter().find(|d| {
            let m = squash(&d.model);
            !m.is_empty() && (m == name || name.contains(&m) || m.contains(&name))
        }) {
            return Some(d);
        }
    }
    (ready.len() == 1).then(|| ready[0])
}

/// Has adb ever paired with this phone (or any)? adb keeps its TLS pairings
/// in `~/.android/adb_known_hosts.pb`, one guid per phone — the same guid
/// the mDNS service is named after. Read, never written.
pub fn known_hosts() -> Vec<u8> {
    std::fs::read(tools::home().join(".android/adb_known_hosts.pb")).unwrap_or_default()
}

pub fn is_known(hosts: &[u8], guid: &str) -> bool {
    !guid.is_empty() && hosts.windows(guid.len()).any(|w| w == guid.as_bytes())
}

// ── the link, as a whole ────────────────────────────────────────────────────

/// Everything the Screen page and `--adb-status` say: the tools, what mDNS
/// sees, whether this computer is paired, which device is the phone.
pub async fn status(phone_name: &str) -> Json {
    let t = tools();
    let mut v = json!({
        "ok": true,
        "tools": tools_json(&t),
        "paired": false,
        "connected": false,
        "serial": "",
        "model": "",
        "services": [],
        "devices": [],
    });
    if t.adb.is_none() {
        return v;
    }
    let hosts = known_hosts();
    let services = match adb(&["mdns", "services"], QUICK).await {
        Ok(o) => parse_services(&o.stdout),
        Err(e) => {
            v["error"] = Json::from(e);
            Vec::new()
        }
    };
    let devices = match adb(&["devices", "-l"], QUICK).await {
        Ok(o) => parse_devices(&o.stdout),
        Err(e) => {
            v["error"] = Json::from(e);
            Vec::new()
        }
    };
    // paired: a phone in reach whose guid adb knows, or — with none in reach
    // right now — any pairing adb has at all
    let paired = services
        .iter()
        .any(|s| s.kind == "connect" && is_known(&hosts, &s.name))
        || devices
            .iter()
            .any(|d| d.state == "device" && d.serial.contains("_adb-tls-connect"))
        || (services.iter().all(|s| s.kind != "connect") && hosts.len() > 8);
    v["paired"] = Json::Bool(paired);
    if let Some(d) = match_device(&devices, phone_name) {
        v["connected"] = Json::Bool(true);
        v["serial"] = Json::from(d.serial.clone());
        v["model"] = Json::from(d.model.replace('_', " "));
    }
    v["services"] = serde_json::to_value(&services).unwrap_or(Json::Array(vec![]));
    v["devices"] = serde_json::to_value(&devices).unwrap_or(Json::Array(vec![]));
    v
}

/// The serial to hand scrcpy, or why there is none (a C3 error code).
pub async fn serial_for(phone_name: &str) -> Result<String, (String, String)> {
    let t = tools();
    if t.adb.is_none() || t.scrcpy.is_none() {
        return Err((
            "no-adb-tools".into(),
            "adb and scrcpy are needed for this. Install android-tools and scrcpy.".into(),
        ));
    }
    let st = status(phone_name).await;
    if st["connected"].as_bool() == Some(true) {
        return Ok(st["serial"].as_str().unwrap_or("").to_string());
    }
    if st["paired"].as_bool() != Some(true) {
        return Err(("adb-not-paired".into(), "This computer isn’t paired with the phone for debugging yet. Open ewe Mobile → Screen to pair.".into()));
    }
    // paired, not connected: try the connect service once
    if let Some(s) = parse_services_json(&st["services"])
        .into_iter()
        .find(|s| s.kind == "connect")
    {
        if connect(&s.host, s.port).await.is_ok() {
            let again = status(phone_name).await;
            if again["connected"].as_bool() == Some(true) {
                return Ok(again["serial"].as_str().unwrap_or("").to_string());
            }
        }
    }
    Err(("adb-not-connected".into(), "The phone isn’t connected for debugging. Switch on Wireless debugging on the phone, on this Wi-Fi.".into()))
}

fn parse_services_json(v: &Json) -> Vec<Service> {
    #[derive(Deserialize)]
    struct S {
        name: String,
        kind: String,
        host: String,
        port: u16,
    }
    serde_json::from_value::<Vec<S>>(v.clone())
        .map(|l| {
            l.into_iter()
                .map(|s| Service {
                    name: s.name,
                    kind: s.kind,
                    host: s.host,
                    port: s.port,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Pair with the six digits the phone shows (Developer options → Wireless
/// debugging → Pair device with pairing code). The phone's pairing service
/// is found by `services`; the person types only the code.
pub async fn pair(host: &str, port: u16, code: &str) -> Result<String, String> {
    if !host_ok(host) {
        return Err("That address isn’t one adb takes.".into());
    }
    let code = code.trim();
    if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err("The pairing code is six digits.".into());
    }
    let addr = format!("{host}:{port}");
    let out = adb(&["pair", &addr, code], NETWORK).await?;
    parse_pair(&out)
}

pub async fn connect(host: &str, port: u16) -> Result<(), String> {
    if !host_ok(host) {
        return Err("That address isn’t one adb takes.".into());
    }
    let addr = format!("{host}:{port}");
    let out = adb(&["connect", &addr], NETWORK).await?;
    parse_connect(&out)
}

pub async fn services() -> Result<Vec<Service>, String> {
    Ok(parse_services(
        &adb(&["mdns", "services"], QUICK).await?.stdout,
    ))
}

/// Was the adb server started by us? `adb start-server` says so once. We
/// stop it on the way out only if we started it and no scrcpy window of
/// ours still needs it (A2: "keeps nothing resident").
pub struct Server(Mutex<bool>);

impl Default for Server {
    fn default() -> Self {
        Self(Mutex::new(false))
    }
}

impl Server {
    pub async fn ensure(&self) {
        if tools().adb.is_none() {
            return;
        }
        if let Ok(o) = adb(&["start-server"], QUICK).await {
            if o.stderr.contains("daemon started successfully")
                || o.stdout.contains("daemon started successfully")
            {
                if let Ok(mut s) = self.0.lock() {
                    *s = true;
                }
            }
        }
    }
    pub fn started_here(&self) -> bool {
        self.0.lock().map(|s| *s).unwrap_or(false)
    }
}

// ── scrcpy ──────────────────────────────────────────────────────────────────

/// What the Screen page offers. Honest options only: each is one scrcpy flag.
#[derive(Debug, Clone, Deserialize)]
pub struct ScreenOpts {
    /// the phone's own screen off while it is mirrored (`-S`)
    #[serde(default)]
    pub screen_off: bool,
    /// the phone's sound on this computer (default on; `--no-audio` off)
    #[serde(default = "yes")]
    pub audio: bool,
    /// the phone stays awake while plugged in (`-w`)
    #[serde(default)]
    pub stay_awake: bool,
    /// `low` | `balanced` | `high` (size and bit rate)
    #[serde(default)]
    pub quality: String,
}

fn yes() -> bool {
    true
}

impl Default for ScreenOpts {
    fn default() -> Self {
        Self {
            screen_off: false,
            audio: true,
            stay_awake: false,
            quality: "balanced".into(),
        }
    }
}

/// The window title scrcpy may be given: one line, no control characters.
fn title_ok(t: &str) -> String {
    let s: String = t.chars().filter(|c| !c.is_control()).collect();
    let s = s.trim();
    if s.is_empty() {
        "Phone".into()
    } else {
        s.chars().take(120).collect()
    }
}

/// Arguments for the mirrored screen (its class is set by `SDL_APP_ID`, not
/// an argument — see `spawn`).
pub fn screen_args(serial: &str, title: &str, o: &ScreenOpts) -> Vec<String> {
    let mut a = vec![
        format!("--serial={serial}"),
        format!("--window-title={}", title_ok(title)),
    ];
    match o.quality.as_str() {
        "low" => a.extend([
            "--max-size=1024".into(),
            "--video-bit-rate=2M".into(),
            "--max-fps=30".into(),
        ]),
        "high" => a.extend(["--video-bit-rate=12M".into()]),
        _ => a.extend(["--max-size=1600".into(), "--video-bit-rate=6M".into()]),
    }
    if o.screen_off {
        a.push("--turn-screen-off".into());
    }
    if o.stay_awake {
        a.push("--stay-awake".into());
    }
    if !o.audio {
        a.push("--no-audio".into());
    }
    a
}

/// Arguments for one app in its own window: a virtual display of its own,
/// resized with the window, no Android bars; `+` stops the app first so one
/// already open on the phone moves here (the probes, RFC-012 §7).
///
/// `--keep-active`: the app's display is in the phone's own display group
/// (Android 17 puts it there), so when the phone falls asleep the window
/// goes black — scrcpy keeps the phone awake while the window is open.
/// `dark` (the phone was asleep or locked, see `wake`): its own screen
/// stays off meanwhile (`--turn-screen-off`), the phone just stays awake.
pub fn app_args(serial: &str, package: &str, title: &str, dark: bool) -> Vec<String> {
    let mut a = vec![
        format!("--serial={serial}"),
        "--new-display=1280x800/160".into(),
        "--flex-display".into(),
        "--no-vd-system-decorations".into(),
        "--keep-active".into(),
    ];
    if dark {
        a.push("--turn-screen-off".into());
    }
    a.extend([
        format!("--start-app=+{package}"),
        format!("--window-title={}", title_ok(title)),
    ]);
    a
}

/// What the phone's screen is doing, from one `adb shell` (the line below
/// is a constant: nothing of ours is ever put into the phone's shell):
/// ```text
///   mWakefulness=Dozing
///     isKeyguardShowing=true
/// ```
/// `(asleep, locked)`. Asleep = Asleep, Dozing or Dreaming. A phone that
/// answers neither line reads as awake and unlocked: nothing is done to it.
pub fn parse_screen_state(text: &str) -> (bool, bool) {
    let mut asleep = false;
    let mut locked = false;
    for l in text.lines().map(str::trim) {
        if let Some(v) = l.strip_prefix("mWakefulness=") {
            if matches!(v, "Asleep" | "Dozing" | "Dreaming") {
                asleep = true;
            }
        } else if l == "isKeyguardShowing=true" {
            locked = true;
        }
    }
    (asleep, locked)
}

const SCREEN_STATE: &str =
    "dumpsys power | grep mWakefulness=; dumpsys window | grep isKeyguardShowing";

/// Before an app opens on a display of its own: the phone has to be awake,
/// or that display is off and the window stays black (the phone asleep on
/// the desk, 2026-11-27). Wakes it when it sleeps; answers whether its own
/// screen may stay dark (asleep or locked: nobody is using the phone).
/// Best effort: when adb cannot tell, nothing is done and the answer is no.
pub async fn wake(serial: &str) -> bool {
    let Ok(out) = adb(&["-s", serial, "shell", SCREEN_STATE], QUICK).await else {
        return false;
    };
    let (asleep, locked) = parse_screen_state(&out.stdout);
    if asleep {
        let _ = adb(
            &["-s", serial, "shell", "input", "keyevent", "KEYCODE_WAKEUP"],
            QUICK,
        )
        .await;
    }
    asleep || locked
}

/// Where a scrcpy window's complaints go (the page reads them back when a
/// window dies): `$XDG_RUNTIME_DIR/ewe-mobile/<class>.log`.
pub fn log_path(class: &str) -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(std::env::temp_dir)
        .join("ewe-mobile");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("{class}.log"))
}

/// Start scrcpy as its own process group — it outlives this app (the
/// window is the person's, not ours) — with `SDL_APP_ID` for its Wayland
/// class (scrcpy 5 is SDL3) and stderr in a log file. No shell anywhere.
pub fn spawn(class: &str, args: &[String]) -> std::io::Result<std::process::Child> {
    use std::os::unix::process::CommandExt;
    let bin = tools()
        .scrcpy
        .ok_or_else(|| std::io::Error::other("scrcpy is not installed"))?;
    let log = std::fs::File::create(log_path(class))?;
    std::process::Command::new(bin)
        .args(args)
        .env("SDL_APP_ID", class)
        .env("SDL_VIDEODRIVER", "wayland")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .process_group(0)
        .spawn()
}

/// The last thing scrcpy said before it ended (for the page), or "".
pub fn last_words(class: &str) -> String {
    let text = std::fs::read_to_string(log_path(class)).unwrap_or_default();
    text.lines()
        .rev()
        .map(str::trim)
        .find(|l| l.starts_with("ERROR") || l.starts_with("WARN") || l.contains("error"))
        .map(|l| {
            l.trim_start_matches("ERROR:")
                .trim_start_matches("WARN:")
                .trim()
                .to_string()
        })
        .unwrap_or_default()
}

/// `scrcpy --list-apps`:
/// ```text
/// List of apps:
///  - Calculator                        com.google.android.calculator
///  * Settings                          com.android.settings
/// ```
/// `-` is an app someone installed, `*` a system app.
pub fn parse_list_apps(text: &str) -> Vec<(String, String, bool)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim_start();
        let Some((mark, rest)) = l.split_at_checked(1) else {
            continue;
        };
        let sys = match mark {
            "-" => false,
            "*" => true,
            _ => continue,
        };
        let rest = rest.trim();
        let Some((name, pkg)) = rest.rsplit_once(char::is_whitespace) else {
            continue;
        };
        let pkg = pkg.trim();
        // an Android package has at least two segments
        if !crate::apps::package_ok(pkg) || !pkg.contains('.') {
            continue;
        }
        let name = crate::apps::clean_name(name);
        out.push((
            if name.is_empty() {
                pkg.to_string()
            } else {
                name
            },
            pkg.to_string(),
            sys,
        ));
    }
    out.sort_by(|a, b| {
        a.0.to_lowercase()
            .cmp(&b.0.to_lowercase())
            .then_with(|| a.1.cmp(&b.1))
    });
    out
}

/// The apps scrcpy knows (the fallback while the phone runs the stock app).
/// Twelve seconds a call: the caller caches it.
pub async fn list_apps(serial: &str) -> Result<Vec<(String, String, bool)>, String> {
    let bin = tools().scrcpy.ok_or_else(|| "no-adb-tools".to_string())?;
    let run = Command::new(bin)
        .args([format!("--serial={serial}"), "--list-apps".into()])
        .env("LANG", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(LIST_APPS, run).await {
        Ok(Ok(o)) if o.status.success() => Ok(parse_list_apps(&String::from_utf8_lossy(&o.stdout))),
        Ok(Ok(o)) => Err(String::from_utf8_lossy(&o.stderr)
            .lines()
            .last()
            .unwrap_or("scrcpy failed")
            .to_string()),
        Ok(Err(e)) => Err(format!("scrcpy didn’t start: {e}")),
        Err(_) => Err("scrcpy took too long to list the apps.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVICES: &str = "List of discovered mdns services\n\
        adb-0A1B2C3D4E5F6G-AbCdEf\t_adb-tls-connect._tcp\t192.168.1.5:39855\n\
        adb-0A1B2C3D4E5F6G-AbCdEf\t_adb-tls-pairing._tcp\t192.168.1.5:41021\n\
        other-thing\t_googlecast._tcp\t192.168.1.9:8009\n\
        bad line\n\
        evil$name\t_adb-tls-connect._tcp\t192.168.1.7:1\n\
        v6\t_adb-tls-connect._tcp\t[fe80::1%wlan0]:5555\n";

    #[test]
    fn mdns_services_are_read() {
        let s = parse_services(SERVICES);
        assert_eq!(s.len(), 3);
        assert_eq!(
            s[0],
            Service {
                name: "adb-0A1B2C3D4E5F6G-AbCdEf".into(),
                kind: "connect".into(),
                host: "192.168.1.5".into(),
                port: 39855
            }
        );
        assert_eq!(s[1].kind, "pairing");
        assert_eq!(s[1].port, 41021);
        assert_eq!(s[2].host, "fe80::1%wlan0");
        assert!(parse_services("List of discovered mdns services\n").is_empty());
    }

    #[test]
    fn devices_are_read_and_matched() {
        let d = parse_devices(
            "* daemon not running; starting now at tcp:5037\n* daemon started successfully\n\
             List of devices attached\n\
             adb-0A1B2C3D4E5F6G-AbCdEf._adb-tls-connect._tcp device product:komodo model:Pixel_9_Pro_XL device:komodo transport_id:3\n\
             192.168.1.20:5555      offline transport_id:4\n\
             emulator-5554          device product:sdk_gphone64 model:sdk_gphone64_x86_64 device:emu64x transport_id:5\n",
        );
        assert_eq!(d.len(), 3);
        assert_eq!(d[0].model, "Pixel_9_Pro_XL");
        assert_eq!(d[1].state, "offline");
        let m = match_device(&d, "Pixel 9 Pro XL").unwrap();
        assert!(m.serial.starts_with("adb-0A1B2"));
        // an unknown name with two ready devices: nobody
        assert!(match_device(&d, "My phone").is_none());
        // one ready device and no name match: the only one
        assert_eq!(
            match_device(&d[..2], "Whatever").unwrap().model,
            "Pixel_9_Pro_XL"
        );
        assert!(
            match_device(&d[1..2], "x").is_none(),
            "an offline device is not a match"
        );
        assert!(parse_devices("List of devices attached\n\n").is_empty());
    }

    #[test]
    fn pair_and_connect_answers() {
        let ok = Out {
            stdout: "Successfully paired to 192.168.1.5:41021 [guid=adb-0A1B2C3D4E5F6G-AbCdEf]\n"
                .into(),
            stderr: String::new(),
        };
        assert_eq!(parse_pair(&ok).unwrap(), "adb-0A1B2C3D4E5F6G-AbCdEf");
        let bad = Out {
            stdout: "Failed: Unable to start pairing client.\n".into(),
            stderr: String::new(),
        };
        assert!(parse_pair(&bad).is_err());
        let wrong = Out {
            stdout: String::new(),
            stderr: "failed to pair to 192.168.1.5:41021\n".into(),
        };
        assert!(parse_pair(&wrong).unwrap_err().contains("six digits"));
        assert!(parse_connect(&Out {
            stdout: "connected to 192.168.1.5:39855\n".into(),
            stderr: String::new()
        })
        .is_ok());
        assert!(parse_connect(&Out {
            stdout: "already connected to 192.168.1.5:39855\n".into(),
            stderr: String::new()
        })
        .is_ok());
        assert!(parse_connect(&Out {
            stdout: "failed to connect to '192.168.1.5:39855': Connection refused\n".into(),
            stderr: String::new()
        })
        .is_err());
        assert!(parse_connect(&Out {
            stdout: "cannot connect to 192.168.1.5:1: Connection refused\n".into(),
            stderr: String::new()
        })
        .is_err());
    }

    #[test]
    fn hosts_and_known_guids() {
        assert!(host_ok("192.168.1.5"));
        assert!(host_ok("fe80::1%wlan0"));
        assert!(host_ok("my-phone.local"));
        assert!(!host_ok(""));
        assert!(!host_ok("-x"));
        assert!(!host_ok("a b"));
        assert!(!host_ok("x;rm"));
        let hosts = b"\n\x1badb-0A1B2C3D4E5F6G-AbCdEf\x12\x00".to_vec();
        assert!(is_known(&hosts, "adb-0A1B2C3D4E5F6G-AbCdEf"));
        assert!(!is_known(&hosts, "adb-OTHER"));
        assert!(!is_known(&hosts, ""));
    }

    #[test]
    fn scrcpy_arguments() {
        let o = ScreenOpts {
            screen_off: true,
            audio: false,
            stay_awake: true,
            quality: "low".into(),
        };
        let a = screen_args("adb-X._adb-tls-connect._tcp", "Pixel 9 Pro XL", &o);
        assert_eq!(a[0], "--serial=adb-X._adb-tls-connect._tcp");
        assert_eq!(a[1], "--window-title=Pixel 9 Pro XL");
        assert!(a.contains(&"--turn-screen-off".to_string()));
        assert!(a.contains(&"--stay-awake".to_string()));
        assert!(a.contains(&"--no-audio".to_string()));
        assert!(a.contains(&"--max-size=1024".to_string()));
        let d = screen_args("s", "a\nb\u{7}", &ScreenOpts::default());
        assert_eq!(d[1], "--window-title=ab");
        assert!(!d.iter().any(|x| x == "--no-audio"), "sound on by default");
        assert!(d.contains(&"--max-size=1600".to_string()));
        assert!(!screen_args(
            "s",
            "t",
            &ScreenOpts {
                quality: "high".into(),
                ..Default::default()
            }
        )
        .iter()
        .any(|x| x.starts_with("--max-size")));

        let a = app_args("s", "com.whatsapp", "WhatsApp", false);
        assert_eq!(
            a,
            vec![
                "--serial=s",
                "--new-display=1280x800/160",
                "--flex-display",
                "--no-vd-system-decorations",
                "--keep-active",
                "--start-app=+com.whatsapp",
                "--window-title=WhatsApp",
            ]
        );
        // every argument is one argv element: nothing is ever joined for a shell
        assert!(a.iter().all(|x| !x.contains('\n')));
        let d = app_args("s", "com.whatsapp", "WhatsApp", true);
        assert!(d.contains(&"--turn-screen-off".to_string()));
        assert!(d.contains(&"--keep-active".to_string()));
    }

    #[test]
    fn screen_state() {
        // a phone asleep on the desk (a Pixel on Android 17)
        assert_eq!(
            parse_screen_state(
                "  mWakefulness=Dozing\nmWakefulness=3\n    isKeyguardShowing=true\n"
            ),
            (true, true)
        );
        assert_eq!(
            parse_screen_state("  mWakefulness=Asleep\n    isKeyguardShowing=false\n"),
            (true, false)
        );
        // awake on the lock screen
        assert_eq!(
            parse_screen_state(
                "  mWakefulness=Awake\nmWakefulness=1\n    isKeyguardShowing=true\n"
            ),
            (false, true)
        );
        // in someone's hand
        assert_eq!(
            parse_screen_state("  mWakefulness=Awake\n    isKeyguardShowing=false\n"),
            (false, false)
        );
        // a phone that says neither: leave it alone
        assert_eq!(parse_screen_state(""), (false, false));
        assert_eq!(parse_screen_state("error: closed\n"), (false, false));
    }

    #[test]
    fn list_apps_output() {
        let l = parse_list_apps(
            "INFO: Connected to adb\nList of apps:\n \
             - Calculator                        com.google.android.calculator\n \
             * Settings                          com.android.settings\n \
             - Alder Bank  mBank                 com.alderbank.mobile\n \
             - Broken line\n \
             - Evil                              not a package!\n",
        );
        assert_eq!(l.len(), 3);
        assert_eq!(
            l[0],
            (
                "Alder Bank mBank".into(),
                "com.alderbank.mobile".into(),
                false
            )
        );
        assert_eq!(
            l[1],
            (
                "Calculator".into(),
                "com.google.android.calculator".into(),
                false
            )
        );
        assert_eq!(
            l[2],
            ("Settings".into(), "com.android.settings".into(), true)
        );
    }

    #[test]
    fn classes_and_logs() {
        assert_eq!(SCREEN_CLASS, "ewe-mobile-screen");
        assert!(log_path("ewe-mobile-screen").ends_with("ewe-mobile/ewe-mobile-screen.log"));
    }
}
