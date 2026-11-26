//! KDE Connect, as ewe Mobile uses it: every command the window can run.
//!
//! The daemon (kdeconnectd) owns the link to the phone, the pairing keys and
//! the caches; this file only asks. Interfaces verified against the running
//! daemon of kdeconnect 26.08 (`busctl --user introspect org.kde.kdeconnect
//! …`). Two facts that are easy to get wrong:
//!
//! - SMS lives on the DEVICE object (`…/devices/<id>`, interface
//!   `org.kde.kdeconnect.device.conversations`), not on a `/conversations`
//!   child, and a message is a STRUCT inside a variant — see `message_json`.
//! - `replyToConversation` silently does nothing for a thread the daemon has
//!   not cached; `send_message` checks first and falls back to the addresses.
//! - The pointer and the keys go out whether or not anyone is listening: a
//!   phone that has not made KDE Connect an accessibility service drops them
//!   without a word. `remoteState` (and the acknowledgement of a key) is the
//!   only thing it ever says back — see `press` and `command`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{json, Value as Json};
use tauri::State;
use tokio::sync::OnceCell;
use zbus::zvariant::{StructureBuilder, Value};

use crate::bus::{Bus, ROOT};
use crate::{contacts, tools};

const DAEMON: &str = "org.kde.kdeconnect.daemon";
const DEVICE: &str = "org.kde.kdeconnect.device";
const BATTERY: &str = "org.kde.kdeconnect.device.battery";
const SIGNAL: &str = "org.kde.kdeconnect.device.connectivity_report";
const NOTIFS: &str = "org.kde.kdeconnect.device.notifications";
const NOTIF: &str = "org.kde.kdeconnect.device.notifications.notification";
const CONV: &str = "org.kde.kdeconnect.device.conversations";
const CONTACTS: &str = "org.kde.kdeconnect.device.contacts";
const MPRIS: &str = "org.kde.kdeconnect.device.mprisremote";
const SHARE: &str = "org.kde.kdeconnect.device.share";
const SFTP: &str = "org.kde.kdeconnect.device.sftp";
const CLIPBOARD: &str = "org.kde.kdeconnect.device.clipboard";
const FIND: &str = "org.kde.kdeconnect.device.findmyphone";
const PING: &str = "org.kde.kdeconnect.device.ping";
const POINTER: &str = "org.kde.kdeconnect.device.remotecontrol";
const KEYBOARD: &str = "org.kde.kdeconnect.device.remotekeyboard";

/// The session bus, opened on first use and kept for the app's life.
#[derive(Default)]
pub struct Link(OnceCell<Bus>);

impl Link {
    pub async fn bus(&self) -> Result<&Bus, String> {
        self.0.get_or_try_init(Bus::connect).await
    }
}

/// Files the daemon itself reported (a received file, a fetched attachment).
/// `open_file` opens only these: the page can never name a path of its own.
#[derive(Default)]
pub struct Allowed(Mutex<HashSet<PathBuf>>);

impl Allowed {
    pub fn allow(&self, p: PathBuf) {
        if let Ok(mut set) = self.0.lock() {
            // a session's worth; the oldest entries are not worth the memory
            if set.len() > 4096 {
                set.clear();
            }
            set.insert(p);
        }
    }
    fn has(&self, p: &Path) -> bool {
        self.0.lock().map(|s| s.contains(p)).unwrap_or(false)
    }
}

/// Files the person handed this window — dropped on it. `share_files` sends
/// only these (the picker's choice never passes through the page at all):
/// what leaves this computer is what someone chose, not what a string named.
#[derive(Default)]
pub struct Offered(Mutex<HashSet<PathBuf>>);

impl Offered {
    pub fn offer(&self, paths: &[PathBuf]) {
        if let Ok(mut set) = self.0.lock() {
            if set.len() > 4096 {
                set.clear();
            }
            set.extend(paths.iter().cloned());
        }
    }
    fn take(&self, p: &Path) -> bool {
        self.0.lock().map(|mut s| s.remove(p)).unwrap_or(false)
    }
}

/// One path element as kdeconnectd writes them (device ids, notification
/// ids): letters, digits, underscore. Anything else never reaches the bus.
fn element_ok(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

pub fn dev_path(id: &str) -> Result<String, String> {
    if !element_ok(id) {
        return Err("That device id is not valid.".into());
    }
    Ok(format!("{ROOT}/devices/{id}"))
}

fn installed() -> bool {
    ["/usr/bin/kdeconnectd", "/usr/lib/kdeconnectd"]
        .iter()
        .any(|p| Path::new(p).is_file())
}

fn strip_plugins(v: &Json) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .map(|s| s.strip_prefix("kdeconnect_").unwrap_or(s).to_string())
                .collect()
        })
        .unwrap_or_default()
}

// ── the daemon and its devices ──────────────────────────────────────────────

pub async fn status_of(bus: &Bus) -> Json {
    let running = bus.daemon_running().await;
    let mut name = String::new();
    let mut id = String::new();
    if running {
        if let Ok(v) = bus.call1(ROOT, DAEMON, "announcedName", &()).await {
            name = v.as_str().unwrap_or("").to_string();
        }
        if let Ok(v) = bus.call1(ROOT, DAEMON, "selfId", &()).await {
            id = v.as_str().unwrap_or("").to_string();
        }
    }
    json!({ "installed": installed(), "running": running, "name": name, "id": id })
}

async fn device_of(bus: &Bus, id: &str) -> Option<Json> {
    let path = dev_path(id).ok()?;
    let p = bus.get_all(&path, DEVICE).await.ok()?;
    let flag = |k: &str| p.get(k).and_then(Json::as_bool).unwrap_or(false);
    let text = |k: &str| p.get(k).and_then(Json::as_str).unwrap_or("").to_string();
    let reachable = flag("isReachable");
    let paired = flag("isPaired");
    let supported = strip_plugins(p.get("supportedPlugins").unwrap_or(&Json::Null));
    let loaded = if reachable && paired {
        bus.call1(&path, DEVICE, "loadedPlugins", &())
            .await
            .map(|v| strip_plugins(&v))
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut battery = Json::Null;
    if loaded.iter().any(|x| x == "battery") {
        if let Ok(b) = bus.get_all(&format!("{path}/battery"), BATTERY).await {
            let charge = b.get("charge").and_then(Json::as_i64).unwrap_or(-1);
            if charge >= 0 && b.get("hasBattery").and_then(Json::as_bool).unwrap_or(true) {
                battery = json!({
                    "charge": charge,
                    "charging": b.get("isCharging").and_then(Json::as_bool).unwrap_or(false),
                });
            }
        }
    }
    let mut signal = Json::Null;
    if loaded.iter().any(|x| x == "connectivity_report") {
        if let Ok(s) = bus
            .get_all(&format!("{path}/connectivity_report"), SIGNAL)
            .await
        {
            let kind = s
                .get("cellularNetworkType")
                .and_then(Json::as_str)
                .unwrap_or("");
            if !kind.is_empty() {
                signal = json!({
                    "type": kind,
                    "strength": s.get("cellularNetworkStrength").and_then(Json::as_i64).unwrap_or(-1),
                });
            }
        }
    }

    let name = text("name");
    Some(json!({
        "id": id,
        "name": if name.is_empty() { id.to_string() } else { name },
        "type": text("type"),
        "reachable": reachable,
        "paired": paired,
        "pairState": p.get("pairState").and_then(Json::as_i64).unwrap_or(0),
        "pairRequested": flag("isPairRequested"),
        "pairRequestedByPeer": flag("isPairRequestedByPeer"),
        "verificationKey": text("verificationKey"),
        "supported": supported,
        "loaded": loaded,
        "battery": battery,
        "signal": signal,
    }))
}

pub async fn devices_of(bus: &Bus) -> Result<Vec<Json>, String> {
    let ids = bus.call1(ROOT, DAEMON, "devices", &(false, false)).await?;
    let mut out = Vec::new();
    for id in ids
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Json::as_str)
    {
        if let Some(d) = device_of(bus, id).await {
            out.push(d);
        }
    }
    // the phone you can use first, then the ones you own, then the rest
    let rank = |d: &Json| {
        let paired = d["paired"].as_bool().unwrap_or(false);
        let reachable = d["reachable"].as_bool().unwrap_or(false);
        match (paired, reachable) {
            (true, true) => 0,
            (true, false) => 1,
            _ => 2,
        }
    };
    out.sort_by(|a, b| {
        rank(a).cmp(&rank(b)).then_with(|| {
            let n = |d: &Json| d["name"].as_str().unwrap_or("").to_lowercase();
            n(a).cmp(&n(b))
        })
    });
    Ok(out)
}

/// What `ewe-mobile --status` prints: one JSON line, never starts the daemon.
pub async fn status_line() -> Json {
    let bus = match Bus::connect().await {
        Ok(b) => b.passive(),
        Err(e) => return json!({ "ok": false, "error": e }),
    };
    let mut v = status_of(&bus).await;
    let devices = if v["running"].as_bool().unwrap_or(false) {
        devices_of(&bus).await.unwrap_or_default()
    } else {
        Vec::new()
    };
    v["ok"] = Json::Bool(true);
    v["devices"] = Json::Array(devices);
    v
}

#[tauri::command]
pub async fn status(link: State<'_, Link>) -> Result<Json, String> {
    Ok(status_of(link.bus().await?).await)
}

#[tauri::command]
pub async fn daemon_start(link: State<'_, Link>) -> Result<bool, String> {
    Ok(link.bus().await?.start_daemon().await)
}

#[tauri::command]
pub async fn devices(link: State<'_, Link>) -> Result<Vec<Json>, String> {
    let bus = link.bus().await?;
    if !bus.daemon_running().await {
        return Ok(Vec::new());
    }
    devices_of(bus).await
}

/// Look again: the daemon re-announces itself on every network it is on.
#[tauri::command]
pub async fn rescan(link: State<'_, Link>) -> Result<(), String> {
    link.bus()
        .await?
        .call(ROOT, DAEMON, "forceOnNetworkChange", &())
        .await
        .map(|_| ())
}

/// The name this computer shows on phones.
#[tauri::command]
pub async fn set_name(link: State<'_, Link>, name: String) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 32 || name.chars().any(char::is_control) {
        return Err("Use a name of 1 to 32 characters.".into());
    }
    link.bus()
        .await?
        .call(ROOT, DAEMON, "setAnnouncedName", &(name,))
        .await
        .map(|_| ())
}

#[tauri::command]
pub async fn pair(link: State<'_, Link>, device: String, action: String) -> Result<(), String> {
    let method = match action.as_str() {
        "request" => "requestPairing",
        "accept" => "acceptPairing",
        "cancel" => "cancelPairing",
        "unpair" => "unpair",
        _ => return Err("unknown pairing action".into()),
    };
    link.bus()
        .await?
        .call(&dev_path(&device)?, DEVICE, method, &())
        .await
        .map(|_| ())
}

/// Every feature both sides support, with whether this computer allows it.
#[tauri::command]
pub async fn plugins(link: State<'_, Link>, device: String) -> Result<Vec<Json>, String> {
    let bus = link.bus().await?;
    let path = dev_path(&device)?;
    let p = bus.get_all(&path, DEVICE).await?;
    let mut out = Vec::new();
    for name in strip_plugins(p.get("supportedPlugins").unwrap_or(&Json::Null)) {
        let full = format!("kdeconnect_{name}");
        let enabled = bus
            .call1(&path, DEVICE, "isPluginEnabled", &(full.as_str(),))
            .await
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        out.push(json!({ "id": name, "enabled": enabled }));
    }
    Ok(out)
}

#[tauri::command]
pub async fn set_plugin(
    link: State<'_, Link>,
    device: String,
    plugin: String,
    enabled: bool,
) -> Result<(), String> {
    if !element_ok(&plugin) {
        return Err("unknown feature".into());
    }
    let full = format!("kdeconnect_{plugin}");
    link.bus()
        .await?
        .call(
            &dev_path(&device)?,
            DEVICE,
            "setPluginEnabled",
            &(full.as_str(), enabled),
        )
        .await
        .map(|_| ())
}

// ── the phone's notifications ───────────────────────────────────────────────

#[tauri::command]
pub async fn notifications(link: State<'_, Link>, device: String) -> Result<Vec<Json>, String> {
    notifications_of(link.bus().await?, &device).await
}

pub async fn notifications_of(bus: &Bus, device: &str) -> Result<Vec<Json>, String> {
    let base = format!("{}/notifications", dev_path(device)?);
    let ids = bus.call1(&base, NOTIFS, "activeNotifications", &()).await?;
    let mut ids: Vec<String> = ids
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Json::as_str)
        .filter(|s| element_ok(s))
        .map(str::to_string)
        .collect();
    // the daemon numbers them as they arrive: newest first
    ids.sort_by_key(|s| std::cmp::Reverse(s.parse::<u64>().unwrap_or(0)));
    let mut out = Vec::new();
    for id in ids {
        let Ok(p) = bus.get_all(&format!("{base}/{id}"), NOTIF).await else {
            continue; // dismissed between the list and the read
        };
        let text = |k: &str| p.get(k).and_then(Json::as_str).unwrap_or("").to_string();
        let flag = |k: &str| p.get(k).and_then(Json::as_bool).unwrap_or(false);
        let icon_path = text("iconPath");
        let icon = if icon_path.is_empty() {
            String::new()
        } else {
            tools::picture_file(Path::new(&icon_path), 512 * 1024)
        };
        out.push(json!({
            "id": id,
            "app": text("appName"),
            "title": text("title"),
            "text": text("text"),
            "ticker": text("ticker"),
            "icon": icon,
            "dismissable": flag("dismissable"),
            "replyId": text("replyId"),
            "silent": flag("silent"),
            "conversation": flag("isConversation"),
        }));
    }
    Ok(out)
}

#[tauri::command]
pub async fn notification_dismiss(
    link: State<'_, Link>,
    device: String,
    id: String,
) -> Result<(), String> {
    if !element_ok(&id) {
        return Err("unknown notification".into());
    }
    let path = format!("{}/notifications/{id}", dev_path(&device)?);
    link.bus()
        .await?
        .call(&path, NOTIF, "dismiss", &())
        .await
        .map(|_| ())
}

#[tauri::command]
pub async fn notification_reply(
    link: State<'_, Link>,
    device: String,
    reply_id: String,
    text: String,
) -> Result<(), String> {
    if reply_id.is_empty() || text.trim().is_empty() {
        return Err("Write a reply first.".into());
    }
    let path = format!("{}/notifications", dev_path(&device)?);
    link.bus()
        .await?
        .call(
            &path,
            NOTIFS,
            "sendReply",
            &(reply_id.as_str(), text.as_str()),
        )
        .await
        .map(|_| ())
}

// ── messages (SMS / MMS) ────────────────────────────────────────────────────

/// One message from the wire. The daemon sends the struct
/// `(i s a(s) x i i x i x a(xsss))`: event, body, addresses, date in ms,
/// type (1 received, 2 sent, 5 failed …), read, thread id, uid, sub (SIM) id,
/// attachments (part id, mime, base64 thumbnail, unique id).
pub fn message_json(v: &Json) -> Option<Json> {
    let f = v.as_array()?;
    if f.len() < 9 {
        return None;
    }
    let addresses: Vec<String> = f[2]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| match a {
            Json::Array(parts) => parts.first().and_then(Json::as_str),
            Json::String(s) => Some(s.as_str()),
            _ => None,
        })
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let attachments: Vec<Json> = f
        .get(9)
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .filter_map(|a| {
            let a = a.as_array()?;
            let thumb = a
                .get(2)
                .and_then(Json::as_str)
                .filter(|s| !s.is_empty())
                .and_then(tools::b64_decode)
                .map(|bytes| tools::data_url(&bytes))
                .unwrap_or_default();
            Some(json!({
                "part": a.first().and_then(Json::as_i64).unwrap_or(0),
                "mime": a.get(1).and_then(Json::as_str).unwrap_or(""),
                "thumb": thumb,
                "uid": a.get(3).and_then(Json::as_str).unwrap_or(""),
            }))
        })
        .collect();
    Some(json!({
        "event": f[0].as_i64().unwrap_or(0),
        "body": f[1].as_str().unwrap_or(""),
        "addresses": addresses,
        "date": f[3].as_i64().unwrap_or(0),
        "type": f[4].as_i64().unwrap_or(1),
        "read": f[5].as_i64().unwrap_or(1) != 0,
        "thread": f[6].as_i64().unwrap_or(-1),
        "uid": f[7].as_i64().unwrap_or(0),
        "sub": f[8].as_i64().unwrap_or(-1),
        "attachments": attachments,
    }))
}

async fn cached_threads(bus: &Bus, path: &str) -> Vec<Json> {
    bus.call1(path, CONV, "activeConversations", &())
        .await
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(message_json)
        .collect()
}

/// The newest message of every thread the daemon has cached, and a request
/// to the phone for the full list — the rest arrive as `message` events.
#[tauri::command]
pub async fn conversations(link: State<'_, Link>, device: String) -> Result<Vec<Json>, String> {
    let bus = link.bus().await?;
    let path = dev_path(&device)?;
    let list = cached_threads(bus, &path).await;
    bus.call(&path, CONV, "requestAllConversationThreads", &())
        .await?;
    Ok(list)
}

/// Ask the phone for messages `start..end` of a thread (newest first); they
/// arrive as `message` events, then `threadLoaded`.
#[tauri::command]
pub async fn conversation_load(
    link: State<'_, Link>,
    device: String,
    thread: i64,
    start: i32,
    end: i32,
) -> Result<(), String> {
    let (start, end) = (start.max(0), end.max(1));
    link.bus()
        .await?
        .call(
            &dev_path(&device)?,
            CONV,
            "requestConversation",
            &(thread, start, end),
        )
        .await
        .map(|_| ())
}

/// A number as someone would type it: digits and the dialling signs.
fn number_ok(s: &str) -> bool {
    let t = s.trim();
    !t.is_empty()
        && t.len() <= 64
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars().all(|c| {
            c.is_ascii_digit() || matches!(c, '+' | '*' | '#' | ' ' | '-' | '(' | ')' | '.')
        })
}

/// An SMS recipient: a number, or a sender name / short code the phone
/// already has a thread with (banks and carriers write from "MyBank").
fn address_ok(s: &str) -> bool {
    let t = s.trim();
    !t.is_empty() && t.chars().count() <= 64 && !t.chars().any(char::is_control)
}

#[tauri::command]
pub async fn send_message(
    link: State<'_, Link>,
    device: String,
    thread: Option<i64>,
    addresses: Vec<String>,
    text: String,
) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("Write a message first.".into());
    }
    let bus = link.bus().await?;
    let path = dev_path(&device)?;
    let no_attachments: Vec<Value<'_>> = Vec::new();

    // a reply stays on the thread's SIM — but only a thread the daemon has
    // cached can be replied to (it drops the call without a word otherwise)
    if let Some(t) = thread {
        let known = cached_threads(bus, &path)
            .await
            .iter()
            .any(|m| m["thread"].as_i64() == Some(t));
        if known {
            return bus
                .call(
                    &path,
                    CONV,
                    "replyToConversation",
                    &(t, text.as_str(), no_attachments),
                )
                .await
                .map(|_| ());
        }
    }

    let to: Vec<&str> = addresses.iter().map(|a| a.trim()).collect();
    if to.is_empty() || !to.iter().all(|a| address_ok(a)) {
        return Err("Choose who to send this to.".into());
    }
    let mut list: Vec<Value<'_>> = Vec::new();
    for a in to {
        // each recipient is the struct (s) inside a variant
        let s = StructureBuilder::new()
            .add_field(a.to_string())
            .build()
            .map_err(|e| e.to_string())?;
        list.push(Value::Structure(s));
    }
    bus.call(
        &path,
        CONV,
        "sendWithoutConversation",
        &(list, text.as_str(), no_attachments),
    )
    .await
    .map(|_| ())
}

/// Fetch a picture or file of a message from the phone; it arrives as an
/// `attachment` event with the path the daemon saved it to.
#[tauri::command]
pub async fn attachment_fetch(
    link: State<'_, Link>,
    device: String,
    part: i64,
    uid: String,
) -> Result<(), String> {
    link.bus()
        .await?
        .call(
            &dev_path(&device)?,
            CONV,
            "requestAttachmentFile",
            &(part, uid.as_str()),
        )
        .await
        .map(|_| ())
}

// ── contacts ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn contacts(device: String) -> Result<Vec<contacts::Contact>, String> {
    dev_path(&device)?;
    tokio::task::spawn_blocking(move || contacts::load(&contacts::dir_for(&device)))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn contact_photo(device: String, uid: String) -> Result<String, String> {
    dev_path(&device)?;
    tokio::task::spawn_blocking(move || contacts::load_photo(&contacts::dir_for(&device), &uid))
        .await
        .map_err(|e| e.to_string())
}

/// Ask the phone for its address book again; a `contacts` event follows.
#[tauri::command]
pub async fn contacts_sync(link: State<'_, Link>, device: String) -> Result<(), String> {
    let path = format!("{}/contacts", dev_path(&device)?);
    link.bus()
        .await?
        .call(&path, CONTACTS, "synchronizeRemoteWithLocal", &())
        .await
        .map(|_| ())
}

// ── what is playing on the phone ────────────────────────────────────────────

#[tauri::command]
pub async fn media(link: State<'_, Link>, device: String) -> Result<Json, String> {
    media_of(link.bus().await?, &device).await
}

pub async fn media_of(bus: &Bus, device: &str) -> Result<Json, String> {
    let path = format!("{}/mprisremote", dev_path(device)?);
    let p = bus.get_all(&path, MPRIS).await?;
    let text = |k: &str| p.get(k).and_then(Json::as_str).unwrap_or("").to_string();
    let num = |k: &str| p.get(k).and_then(Json::as_i64).unwrap_or(0);
    let art = tools::path_of_url(&text("localAlbumArtUrl"))
        .map(|f| tools::picture_file(&f, 4 * 1024 * 1024))
        .unwrap_or_default();
    Ok(json!({
        "player": text("player"),
        "players": p.get("playerList").cloned().unwrap_or_else(|| json!([])),
        "title": text("title"),
        "artist": text("artist"),
        "album": text("album"),
        "playing": p.get("isPlaying").and_then(Json::as_bool).unwrap_or(false),
        "canSeek": p.get("canSeek").and_then(Json::as_bool).unwrap_or(false),
        "length": num("length"),
        "position": num("position"),
        "volume": num("volume"),
        "art": art,
    }))
}

#[tauri::command]
pub async fn media_action(
    link: State<'_, Link>,
    device: String,
    action: String,
) -> Result<(), String> {
    if !matches!(
        action.as_str(),
        "Play" | "Pause" | "PlayPause" | "Next" | "Previous" | "Stop"
    ) {
        return Err("unknown player action".into());
    }
    let path = format!("{}/mprisremote", dev_path(&device)?);
    link.bus()
        .await?
        .call(&path, MPRIS, "sendAction", &(action.as_str(),))
        .await
        .map(|_| ())
}

/// `volume` (0–100), `position` (ms) or `player` (one of the player list).
#[tauri::command]
pub async fn media_set(
    link: State<'_, Link>,
    device: String,
    what: String,
    value: Json,
) -> Result<(), String> {
    let path = format!("{}/mprisremote", dev_path(&device)?);
    let bus = link.bus().await?;
    match what.as_str() {
        "volume" => {
            let v = value.as_i64().ok_or("volume is a number")?.clamp(0, 100) as i32;
            bus.set(&path, MPRIS, "volume", Value::from(v)).await
        }
        "position" => {
            let v = value.as_i64().ok_or("position is a number")?.max(0);
            let v = i32::try_from(v).map_err(|_| "position is out of range")?;
            bus.set(&path, MPRIS, "position", Value::from(v)).await
        }
        "player" => {
            let v = value.as_str().ok_or("player is a name")?;
            bus.set(&path, MPRIS, "player", Value::from(v)).await
        }
        _ => Err("unknown player setting".into()),
    }
}

// ── sharing, finding, the clipboard ─────────────────────────────────────────

async fn send_files(bus: &Bus, device: &str, files: Vec<PathBuf>) -> Result<usize, String> {
    let urls: Vec<String> = files
        .iter()
        .filter(|p| p.is_absolute() && p.is_file())
        .map(|p| tools::file_url(p))
        .collect();
    if urls.is_empty() {
        return Err("Choose at least one file. Folders can’t be sent.".into());
    }
    let path = format!("{}/share", dev_path(device)?);
    bus.call(&path, SHARE, "shareUrls", &(urls.clone(),))
        .await?;
    Ok(urls.len())
}

/// Send the files that were dropped on the window — and only those.
#[tauri::command]
pub async fn share_files(
    link: State<'_, Link>,
    offered: State<'_, Offered>,
    device: String,
    paths: Vec<String>,
) -> Result<usize, String> {
    let files: Vec<PathBuf> = paths
        .iter()
        .map(PathBuf::from)
        .filter(|p| offered.take(p))
        .collect();
    send_files(link.bus().await?, &device, files).await
}

/// Ask which files to send (ewe's file picker, through the portal) and send
/// them. 0 when the picker was closed without a choice.
#[tauri::command]
pub async fn share_pick(
    app: tauri::AppHandle,
    link: State<'_, Link>,
    device: String,
) -> Result<usize, String> {
    use tauri_plugin_dialog::DialogExt;
    dev_path(&device)?;
    let picked = tokio::task::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Send to phone")
            .blocking_pick_files()
    })
    .await
    .map_err(|e| e.to_string())?;
    let files: Vec<PathBuf> = picked
        .unwrap_or_default()
        .into_iter()
        .filter_map(|f| f.into_path().ok())
        .collect();
    if files.is_empty() {
        return Ok(0);
    }
    send_files(link.bus().await?, &device, files).await
}

/// Text lands in the phone's clipboard; a link opens in its browser.
#[tauri::command]
pub async fn share_text(
    link: State<'_, Link>,
    device: String,
    text: String,
) -> Result<String, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("Write or paste something first.".into());
    }
    let path = format!("{}/share", dev_path(&device)?);
    let bus = link.bus().await?;
    let is_link = (t.starts_with("https://") || t.starts_with("http://"))
        && !t.chars().any(char::is_whitespace);
    if is_link {
        bus.call(&path, SHARE, "shareUrl", &(t,)).await?;
        Ok("link".into())
    } else {
        bus.call(&path, SHARE, "shareText", &(t,)).await?;
        Ok("text".into())
    }
}

/// Open the phone's dialler on a number (a `tel:` link through share — the
/// phone asks before it calls).
#[tauri::command]
pub async fn dial(link: State<'_, Link>, device: String, number: String) -> Result<(), String> {
    if !number_ok(&number) {
        return Err("That doesn’t look like a phone number.".into());
    }
    let digits: String = number
        .chars()
        .filter(|c| c.is_ascii_digit() || matches!(c, '+' | '*' | '#'))
        .collect();
    let url = format!("tel:{}", digits.replace('#', "%23"));
    let path = format!("{}/share", dev_path(&device)?);
    link.bus()
        .await?
        .call(&path, SHARE, "shareUrl", &(url.as_str(),))
        .await
        .map(|_| ())
}

/// One of the device's simple actions: ring it, ping it, send it this
/// computer's clipboard.
#[tauri::command]
pub async fn act(link: State<'_, Link>, device: String, action: String) -> Result<(), String> {
    let base = dev_path(&device)?;
    let (sub, iface, method) = match action.as_str() {
        "ring" => ("findmyphone", FIND, "ring"),
        "ping" => ("ping", PING, "sendPing"),
        "clipboard" => ("clipboard", CLIPBOARD, "sendClipboard"),
        _ => return Err("unknown action".into()),
    };
    link.bus()
        .await?
        .call(&format!("{base}/{sub}"), iface, method, &())
        .await
        .map(|_| ())
}

// ── the phone's storage (SFTP) ──────────────────────────────────────────────

fn open_folder(path: &str) -> bool {
    if tools::which("ewe-files-app") {
        tools::spawn_reaped("ewe-files-app", &[path])
    } else {
        tools::spawn_reaped("xdg-open", &[path])
    }
}

#[tauri::command]
pub async fn storage_state(link: State<'_, Link>, device: String) -> Result<Json, String> {
    let path = format!("{}/sftp", dev_path(&device)?);
    let bus = link.bus().await?;
    let mounted = bus
        .call1(&path, SFTP, "isMounted", &())
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    Ok(json!({ "mounted": mounted, "sshfs": tools::which("sshfs") }))
}

/// Mount the phone's storage and open it in ewe Files.
#[tauri::command]
pub async fn storage_open(link: State<'_, Link>, device: String) -> Result<String, String> {
    if !tools::which("sshfs") {
        return Err(
            "Browsing the phone needs sshfs. Install it with Komble, then try again.".into(),
        );
    }
    let path = format!("{}/sftp", dev_path(&device)?);
    let bus = link.bus().await?;
    // waits for the phone: it has to be unlocked and answer
    let ok = bus
        .call_within(crate::bus::PATIENT, &path, SFTP, "mountAndWait", &())
        .await?
        .first()
        .and_then(Json::as_bool)
        .unwrap_or(false);
    if !ok {
        let why = bus
            .call1(&path, SFTP, "getMountError", &())
            .await
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
        return Err(if why.is_empty() {
            "Couldn’t open the phone’s storage. Unlock the phone and allow file access in KDE Connect.".into()
        } else {
            format!("Couldn’t open the phone’s storage: {why}")
        });
    }
    // the folders the phone shares (Internal storage, an SD card…), else the
    // mount itself
    let dirs = bus
        .call1(&path, SFTP, "getDirectories", &())
        .await
        .unwrap_or(Json::Null);
    let mut target = dirs
        .as_object()
        .and_then(|m| m.keys().min().cloned())
        .unwrap_or_default();
    if target.is_empty() {
        target = bus
            .call1(&path, SFTP, "mountPoint", &())
            .await?
            .as_str()
            .unwrap_or("")
            .to_string();
    }
    if !Path::new(&target).is_absolute() || !Path::new(&target).is_dir() {
        return Err(
            "The phone’s storage is connected, but its folder isn’t there yet. Try again.".into(),
        );
    }
    if !open_folder(&target) {
        return Err("Couldn’t open a file manager.".into());
    }
    Ok(target)
}

#[tauri::command]
pub async fn storage_close(link: State<'_, Link>, device: String) -> Result<(), String> {
    let path = format!("{}/sftp", dev_path(&device)?);
    link.bus()
        .await?
        .call(&path, SFTP, "unmount", &())
        .await
        .map(|_| ())
}

// ── typing and pointing on the phone ────────────────────────────────────────

/// KDE Connect's key numbers (its protocol, not Qt's or the kernel's).
fn special_key(name: &str) -> Option<i32> {
    Some(match name {
        "Backspace" => 1,
        "Tab" => 2,
        "ArrowLeft" => 4,
        "ArrowUp" => 5,
        "ArrowRight" => 6,
        "ArrowDown" => 7,
        "PageUp" => 8,
        "PageDown" => 9,
        "Home" => 10,
        "End" => 11,
        "Enter" => 12,
        "Delete" => 13,
        "Escape" => 14,
        _ => return None,
    })
}

/// The modifiers held with a key.
#[derive(Clone, Copy, Default)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

/// `sendKeyPress(key, specialKey, shift, ctrl, alt, super, sendAck)` — seven
/// arguments since kdeconnect 26.08 (`super` came in before `sendAck`; with
/// six, the last one is read as `super`). The acknowledgement is asked for:
/// the phone answers every key it really typed with `keyPressReceived`, the
/// only word the page ever gets that typing works (events.rs, `keyEcho`).
pub async fn press(
    bus: &Bus,
    device: &str,
    text: &str,
    special: i32,
    mods: Mods,
) -> Result<(), String> {
    let path = format!("{}/remotekeyboard", dev_path(device)?);
    bus.call(
        &path,
        KEYBOARD,
        "sendKeyPress",
        &(
            text, special, mods.shift, mods.ctrl, mods.alt, mods.meta, true,
        ),
    )
    .await
    .map(|_| ())
}

/// One key: `key` is a KeyboardEvent.key — a character, or a named key.
#[tauri::command]
pub async fn key_send(
    link: State<'_, Link>,
    device: String,
    key: String,
    shift: bool,
    ctrl: bool,
    alt: bool,
    meta: bool,
) -> Result<(), String> {
    let (text, special) = match special_key(&key) {
        Some(code) => (String::new(), code),
        None if key.chars().count() == 1 && !key.chars().any(char::is_control) => (key, 0),
        None => return Ok(()), // F-keys, modifiers alone: nothing the phone takes
    };
    let mods = Mods {
        shift,
        ctrl,
        alt,
        meta,
    };
    press(link.bus().await?, &device, &text, special, mods).await
}

/// The longest run of text one packet carries.
pub const TEXT_MAX: usize = 500;

/// A run of text, typed as one piece (a paste): the phone commits the whole
/// string at once, so it cannot arrive out of order or half-typed. One line
/// at a time — a line break is the Enter key, which is `key_send`'s.
#[tauri::command]
pub async fn text_send(link: State<'_, Link>, device: String, text: String) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }
    if text.chars().count() > TEXT_MAX || text.chars().any(char::is_control) {
        return Err("That text can’t be typed on the phone.".into());
    }
    press(link.bus().await?, &device, &text, 0, Mods::default()).await
}

/// Is the phone's KDE Connect keyboard up and taking keys?
#[tauri::command]
pub async fn keyboard_state(link: State<'_, Link>, device: String) -> Result<bool, String> {
    let path = format!("{}/remotekeyboard", dev_path(&device)?);
    Ok(link
        .bus()
        .await?
        .get_all(&path, KEYBOARD)
        .await
        .ok()
        .and_then(|p| p.get("remoteState").and_then(Json::as_bool))
        .unwrap_or(false))
}

/// Move the pointer by (`dx`, `dy`): `moveCursor` takes one QPoint, `(ii)`.
pub async fn point(bus: &Bus, device: &str, dx: i32, dy: i32) -> Result<(), String> {
    let (dx, dy) = (dx.clamp(-2000, 2000), dy.clamp(-2000, 2000));
    if dx == 0 && dy == 0 {
        return Ok(());
    }
    let path = format!("{}/remotecontrol", dev_path(device)?);
    bus.call(&path, POINTER, "moveCursor", &((dx, dy),))
        .await
        .map(|_| ())
}

#[tauri::command]
pub async fn pointer_move(
    link: State<'_, Link>,
    device: String,
    dx: i32,
    dy: i32,
) -> Result<(), String> {
    point(link.bus().await?, &device, dx, dy).await
}

/// The words of `kdeconnect.mousepad.request` a pointer may say. What a
/// phone makes of them (KDE Connect for Android, MouseReceiverPlugin):
/// `singleclick` taps where the pointer is, `singlehold` puts the finger
/// down and `singlerelease` lifts it, `middleclick` is Home, `backclick`
/// Back, `forwardclick` and `doubleclick` Recents, `rightclick` the menu key
/// on Android 16 and Back before it.
const POINTER_WORDS: [&str; 8] = [
    "singleclick",
    "doubleclick",
    "middleclick",
    "rightclick",
    "backclick",
    "forwardclick",
    "singlehold",
    "singlerelease",
];

/// The farthest one scroll message may say. A phone adds `dy` up and turns
/// one page when the sum passes 500 (KdeConnectAccessibilityService.scroll),
/// so a whole page is one message of 500 or a little more.
const SCROLL_MAX: f64 = 1200.0;

/// A click or one of the phone's buttons, or a scroll by (`dx`, `dy`).
pub async fn command(
    bus: &Bus,
    device: &str,
    action: &str,
    dx: f64,
    dy: f64,
) -> Result<(), String> {
    let mut body: HashMap<&str, Value<'_>> = HashMap::new();
    if action == "scroll" {
        let fit = |v: f64| {
            if v.is_finite() {
                v.clamp(-SCROLL_MAX, SCROLL_MAX)
            } else {
                0.0
            }
        };
        body.insert("scroll", Value::from(true));
        body.insert("dx", Value::from(fit(dx)));
        body.insert("dy", Value::from(fit(dy)));
    } else if POINTER_WORDS.contains(&action) {
        body.insert(action, Value::from(true));
    } else {
        return Err("unknown pointer action".into());
    }
    let path = format!("{}/remotecontrol", dev_path(device)?);
    bus.call(&path, POINTER, "sendCommand", &(body,))
        .await
        .map(|_| ())
}

#[tauri::command]
pub async fn pointer_do(
    link: State<'_, Link>,
    device: String,
    action: String,
    dx: Option<f64>,
    dy: Option<f64>,
) -> Result<(), String> {
    command(
        link.bus().await?,
        &device,
        &action,
        dx.unwrap_or(0.0),
        dy.unwrap_or(0.0),
    )
    .await
}

// ── opening things on this computer ─────────────────────────────────────────

/// Open a file the daemon reported (a received file, a fetched attachment).
#[tauri::command]
pub async fn open_file(
    allowed: State<'_, Allowed>,
    path: String,
    reveal: bool,
) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !allowed.has(&p) {
        return Err("That file didn’t come from the phone in this session.".into());
    }
    if !p.exists() {
        return Err("That file has been moved or deleted.".into());
    }
    let ok = if reveal {
        if tools::which("ewe-files-app") {
            tools::spawn_reaped("ewe-files-app", &[&format!("--select={path}")])
        } else {
            let dir = p.parent().unwrap_or(Path::new("/")).display().to_string();
            tools::spawn_reaped("xdg-open", &[&dir])
        }
    } else {
        tools::spawn_reaped("xdg-open", &[&path])
    };
    if ok {
        Ok(())
    } else {
        Err("Couldn’t open that file.".into())
    }
}

/// Open a web link from a message in the browser.
#[tauri::command]
pub async fn open_link(url: String) -> Result<(), String> {
    let ok = (url.starts_with("https://") || url.starts_with("http://"))
        && url.len() <= 2048
        && !url.chars().any(|c| c.is_whitespace() || c.is_control());
    if !ok {
        return Err("Only web links can be opened.".into());
    }
    if tools::spawn_reaped("xdg-open", &[&url]) {
        Ok(())
    } else {
        Err("Couldn’t open the browser.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_from_the_wire() {
        // the struct the daemon sends, as bus::to_json renders it
        let wire = json!([
            1,
            "See you at 6",
            [["+12025550101"]],
            1764000000000i64,
            1,
            0,
            12,
            345,
            1,
            [[7, "image/jpeg", "/9j/4AAQ", "PART_7"]]
        ]);
        let m = message_json(&wire).unwrap();
        assert_eq!(m["body"], "See you at 6");
        assert_eq!(m["addresses"], json!(["+12025550101"]));
        assert_eq!(m["date"], 1764000000000i64);
        assert_eq!(m["type"], 1);
        assert_eq!(m["read"], false);
        assert_eq!(m["thread"], 12);
        assert_eq!(m["attachments"][0]["part"], 7);
        assert_eq!(m["attachments"][0]["uid"], "PART_7");
        assert!(m["attachments"][0]["thumb"]
            .as_str()
            .unwrap()
            .starts_with("data:image/jpeg;base64,"));
    }

    #[test]
    fn a_group_message_and_a_short_struct() {
        let wire = json!([3, "", [["+1"], ["+2"]], 5, 2, 1, 9, 1, -1, []]);
        let m = message_json(&wire).unwrap();
        assert_eq!(m["addresses"], json!(["+1", "+2"]));
        assert_eq!(m["type"], 2);
        assert_eq!(m["attachments"], json!([]));
        // an attachment whose thumbnail is not a picture shows none
        let odd = json!([
            1,
            "",
            [["+1"]],
            5,
            1,
            1,
            9,
            1,
            -1,
            [[1, "text/x-vcard", "QkVHSU4=", "u"]]
        ]);
        assert_eq!(message_json(&odd).unwrap()["attachments"][0]["thumb"], "");
        assert!(message_json(&json!([1, "x"])).is_none());
        assert!(message_json(&json!({"body": "dict"})).is_none());
    }

    #[test]
    fn only_bus_safe_ids_reach_a_path() {
        assert!(dev_path("0123456789abcdef0123456789abcdef").is_ok());
        assert!(dev_path("a_b_C9").is_ok());
        assert!(dev_path("").is_err());
        assert!(dev_path("a/b").is_err());
        assert!(dev_path("../x").is_err());
        assert!(dev_path("a-b").is_err());
    }

    #[test]
    fn numbers_and_recipients() {
        assert!(number_ok("+1 202 555 0101"));
        assert!(number_ok("(032) 2-00-00-02"));
        assert!(number_ok("*100#"));
        assert!(!number_ok("MyBank"));
        assert!(!number_ok(""));
        assert!(!number_ok("tel:1;rm -rf"));
        assert!(address_ok("MyBank"));
        assert!(!address_ok("  "));
        assert!(!address_ok("a\nb"));
    }

    /// Reads the real daemon and the real phone — counts and shapes only,
    /// nothing is sent and no content is printed. Off by default:
    ///   cargo test live_reads -- --ignored --nocapture
    #[test]
    #[ignore = "needs a running kdeconnectd with a paired, reachable phone"]
    fn live_reads() {
        tauri::async_runtime::block_on(async {
            let bus = Bus::connect().await.unwrap().passive();
            let st = status_of(&bus).await;
            assert_eq!(st["running"], true, "kdeconnectd is not running");
            let devs = devices_of(&bus).await.unwrap();
            let dev = devs
                .iter()
                .find(|d| d["paired"] == true && d["reachable"] == true)
                .expect("no paired, reachable device");
            let id = dev["id"].as_str().unwrap();
            println!(
                "device: type={} battery={} signal={}",
                dev["type"], dev["battery"], dev["signal"]
            );

            let notes = notifications_of(&bus, id).await.unwrap();
            let with_icon = notes
                .iter()
                .filter(|n| {
                    n["icon"]
                        .as_str()
                        .is_some_and(|i| i.starts_with("data:image/"))
                })
                .count();
            let replyable = notes
                .iter()
                .filter(|n| n["replyId"].as_str().is_some_and(|r| !r.is_empty()))
                .count();
            println!(
                "notifications: {} ({} with an icon, {} can be replied to)",
                notes.len(),
                with_icon,
                replyable
            );
            assert!(notes
                .iter()
                .all(|n| n["app"].is_string() && n["id"].is_string()));

            let threads = cached_threads(&bus, &dev_path(id).unwrap()).await;
            println!("threads cached by the daemon: {}", threads.len());
            for m in &threads {
                assert!(m["thread"].as_i64().unwrap() >= 0);
                assert!(m["date"].as_i64().unwrap() > 1_000_000_000_000);
                assert!(!m["addresses"].as_array().unwrap().is_empty());
            }

            let book = contacts::load(&contacts::dir_for(id));
            let photos = book.iter().filter(|c| c.has_photo).count();
            println!("contacts: {} ({} with a photo)", book.len(), photos);
            assert!(book.iter().all(|c| !c.name.is_empty()));
            if let Some(c) = book.iter().find(|c| c.has_photo) {
                let url = contacts::load_photo(&contacts::dir_for(id), &c.uid);
                assert!(url.starts_with("data:image/"), "a photo did not decode");
            }

            let m = media_of(&bus, id).await.unwrap();
            println!(
                "media: players={} playing={} length={} art={}",
                m["players"].as_array().map(Vec::len).unwrap_or(0),
                m["playing"],
                m["length"],
                m["art"]
                    .as_str()
                    .is_some_and(|a| a.starts_with("data:image/"))
            );
        });
    }

    #[test]
    fn named_keys() {
        assert_eq!(special_key("Enter"), Some(12));
        assert_eq!(special_key("Backspace"), Some(1));
        assert_eq!(special_key("a"), None);
        assert_eq!(special_key("F5"), None);
    }

    /// A stand-in for kdeconnectd with one phone's pointer and keyboard on
    /// it. Every call is printed with the signature it arrived with — the
    /// real daemon (Qt) refuses a call whose types are not exactly its own,
    /// and dbus-python alone would take anything. A key that asks for an
    /// acknowledgement gets one, as a phone that typed it gives.
    const PHONE: &str = r#"
import json, dbus, dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib
DBusGMainLoop(set_as_default=True)
bus = dbus.SessionBus()
name = dbus.service.BusName("org.kde.kdeconnect", bus)
DEV = "/modules/kdeconnect/devices/abc123"
POINTER = "org.kde.kdeconnect.device.remotecontrol"
KEYS = "org.kde.kdeconnect.device.remotekeyboard"
def say(**kw):
    print(json.dumps(kw), flush=True)
def plain(v):
    return bool(v) if isinstance(v, dbus.Boolean) else float(v)
class Pointer(dbus.service.Object):
    @dbus.service.method(POINTER, in_signature="(ii)", message_keyword="msg")
    def moveCursor(self, p, msg=None):
        say(call="move", sig=msg.get_signature(), dx=int(p[0]), dy=int(p[1]))
    @dbus.service.method(POINTER, in_signature="a{sv}", message_keyword="msg")
    def sendCommand(self, body, msg=None):
        say(call="command", sig=msg.get_signature(), body={str(k): plain(v) for k, v in body.items()})
class Keys(dbus.service.Object):
    @dbus.service.method(KEYS, in_signature="sibbbbb", message_keyword="msg")
    def sendKeyPress(self, key, special, shift, ctrl, alt, sup, ack, msg=None):
        say(call="key", sig=msg.get_signature(), key=str(key), special=int(special),
            shift=bool(shift), ctrl=bool(ctrl), alt=bool(alt), super=bool(sup), ack=bool(ack))
        if ack:
            self.keyPressReceived(key, special, shift, ctrl, alt, sup)
    @dbus.service.signal(KEYS, signature="sibbbb")
    def keyPressReceived(self, key, special, shift, ctrl, alt, sup):
        pass
pointer = Pointer(bus, DEV + "/remotecontrol")
keys = Keys(bus, DEV + "/remotekeyboard")
GLib.MainLoop().run()
"#;

    /// What the Remote page sends, as the daemon receives it: the signatures
    /// kdeconnect 26.08 has, the values in their places, and the phone's
    /// acknowledgement of a key back as a `keyEcho` event. Run by
    /// `tests/remote.sh` on a private bus; refuses to run anywhere else.
    #[test]
    #[ignore = "run through tests/remote.sh (a private session bus)"]
    fn private_bus_remote() {
        use std::process::{Command, Stdio};
        use std::sync::{Arc, Mutex};
        assert_eq!(
            std::env::var("EWE_MOBILE_TEST_BUS").as_deref(),
            Ok("private"),
            "run tests/remote.sh — this must never run on the real session bus"
        );
        let mut phone = Command::new("python3")
            .args(["-c", PHONE])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()
            .expect("python3 with dbus and gi");
        let heard = tauri::async_runtime::block_on(async {
            let bus = Bus::connect().await.unwrap().passive();
            for _ in 0..50 {
                if bus.owner().await.is_some() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            assert!(
                bus.owner().await.is_some(),
                "the stand-in daemon never took its name"
            );
            let echoes: Arc<Mutex<Vec<Json>>> = Arc::default();
            let sink = echoes.clone();
            let pump_bus = bus.clone();
            let task = tauri::async_runtime::spawn(async move {
                let _ = crate::events::pump(pump_bus, move |ev| {
                    if ev["kind"] == "keyEcho" {
                        sink.lock().unwrap().push(ev);
                    }
                })
                .await;
            });
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;

            let dev = "abc123";
            point(&bus, dev, 12, -7).await.unwrap();
            point(&bus, dev, 0, 0).await.unwrap(); // nothing to say: not sent
            command(&bus, dev, "singleclick", 0.0, 0.0).await.unwrap();
            command(&bus, dev, "backclick", 0.0, 0.0).await.unwrap();
            command(&bus, dev, "singlehold", 0.0, 0.0).await.unwrap();
            command(&bus, dev, "singlerelease", 0.0, 0.0).await.unwrap();
            command(&bus, dev, "scroll", 0.0, 600.0).await.unwrap();
            command(&bus, dev, "scroll", 0.0, f64::NAN).await.unwrap();
            assert!(command(&bus, dev, "reboot", 0.0, 0.0).await.is_err());
            assert!(command(&bus, "../x", "singleclick", 0.0, 0.0)
                .await
                .is_err());
            let shifted = Mods {
                shift: true,
                ..Mods::default()
            };
            press(&bus, dev, "A", 0, shifted).await.unwrap();
            press(&bus, dev, "", 12, Mods::default()).await.unwrap();
            press(&bus, dev, "see you at 6", 0, Mods::default())
                .await
                .unwrap();

            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            task.abort();
            let n = echoes.lock().unwrap().clone();
            n
        });
        let _ = phone.kill();
        let out = phone.wait_with_output().expect("the stand-in's output");
        let calls: Vec<Json> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        println!(
            "calls the daemon got: {}; keys acknowledged: {}",
            calls.len(),
            heard.len()
        );

        assert_eq!(
            calls[0],
            json!({ "call": "move", "sig": "(ii)", "dx": 12, "dy": -7 })
        );
        let commands: Vec<&Json> = calls.iter().filter(|c| c["call"] == "command").collect();
        assert!(commands.iter().all(|c| c["sig"] == "a{sv}"));
        let words: Vec<String> = commands
            .iter()
            .map(|c| {
                let mut keys: Vec<&String> = c["body"].as_object().unwrap().keys().collect();
                keys.sort();
                keys.iter()
                    .map(|k| k.as_str())
                    .collect::<Vec<_>>()
                    .join("+")
            })
            .collect();
        assert_eq!(
            words,
            [
                "singleclick",
                "backclick",
                "singlehold",
                "singlerelease",
                "dx+dy+scroll",
                "dx+dy+scroll"
            ]
        );
        assert_eq!(commands[0]["body"]["singleclick"], true);
        assert_eq!(commands[4]["body"]["dy"], 600.0);
        assert_eq!(commands[5]["body"]["dy"], 0.0); // not a number: no scroll

        let keys: Vec<&Json> = calls.iter().filter(|c| c["call"] == "key").collect();
        assert_eq!(keys.len(), 3);
        assert!(keys.iter().all(|k| k["sig"] == "sibbbbb"));
        assert!(keys.iter().all(|k| k["ack"] == true && k["super"] == false));
        assert_eq!(keys[0]["key"], "A");
        assert_eq!(keys[0]["shift"], true);
        assert_eq!(keys[1]["key"], "");
        assert_eq!(keys[1]["special"], 12);
        assert_eq!(keys[2]["key"], "see you at 6");
        // and nothing else was called
        assert_eq!(calls.len(), 1 + commands.len() + keys.len());

        assert_eq!(heard.len(), 3, "every key the phone typed is answered");
        assert!(heard.iter().all(|e| e["device"] == "abc123"));
    }
}
