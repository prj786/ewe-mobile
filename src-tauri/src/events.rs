//! What the daemon says by itself: every signal under `/modules/kdeconnect`
//! becomes one `kdc` event for the window — `{ kind, device, … }`.
//!
//! One match rule for the whole tree instead of a subscription per device
//! and plugin: the device id is read off the object path, so a phone that
//! appears later needs nothing set up. Most events only say "this changed"
//! and the page asks for the new state; the ones that carry their payload
//! (a message, the battery) do because the daemon offers no way to ask.

use futures_util::StreamExt;
use serde_json::{json, Value as Json};
use tauri::{AppHandle, Emitter, Manager};
use zbus::zvariant::Structure;

use crate::bus::{to_json, Bus, ROOT, SERVICE};
use crate::kdc::{self, Allowed, Link};
use crate::tools;

pub const EVENT: &str = "kdc";

/// `/modules/kdeconnect/devices/<id>/…` → `<id>`.
fn device_of(path: &str) -> &str {
    path.strip_prefix(ROOT)
        .and_then(|p| p.strip_prefix("/devices/"))
        .and_then(|p| p.split('/').next())
        .unwrap_or("")
}

/// One signal → the event the window gets (None: nothing it shows changed).
pub fn event_for(iface: &str, member: &str, path: &str, args: &[Json]) -> Option<Json> {
    let device = device_of(path);
    let text = |i: usize| args.get(i).and_then(Json::as_str).unwrap_or("");
    let short = iface.strip_prefix("org.kde.kdeconnect.")?;
    Some(match (short, member) {
        ("daemon", "announcedNameChanged") => json!({ "kind": "self" }),
        ("daemon", _) => json!({ "kind": "devices" }),
        ("device", "pairingFailed") => {
            json!({ "kind": "pairError", "device": device, "error": text(0) })
        }
        ("device", _) => json!({ "kind": "devices" }),
        ("device.battery", "refreshed") => json!({
            "kind": "battery",
            "device": device,
            "charging": args.first().and_then(Json::as_bool).unwrap_or(false),
            "charge": args.get(1).and_then(Json::as_i64).unwrap_or(-1),
        }),
        ("device.connectivity_report", "refreshed") => json!({
            "kind": "signal",
            "device": device,
            "type": text(0),
            "strength": args.get(1).and_then(Json::as_i64).unwrap_or(-1),
        }),
        ("device.notifications", _) | ("device.notifications.notification", "ready") => {
            json!({ "kind": "notifications", "device": device })
        }
        ("device.conversations", "conversationCreated" | "conversationUpdated") => {
            let msg = kdc::message_json(args.first()?)?;
            json!({ "kind": "message", "device": device, "msg": msg })
        }
        ("device.conversations", "conversationRemoved") => json!({
            "kind": "threadRemoved",
            "device": device,
            "thread": args.first().and_then(Json::as_i64).unwrap_or(-1),
        }),
        ("device.conversations", "conversationLoaded") => json!({
            "kind": "threadLoaded",
            "device": device,
            "thread": args.first().and_then(Json::as_i64).unwrap_or(-1),
            "count": args.get(1).and_then(Json::as_u64).unwrap_or(0),
        }),
        ("device.conversations", "attachmentReceived") => json!({
            "kind": "attachment",
            "device": device,
            "path": text(0),
            "name": text(1),
        }),
        ("device.contacts", "localCacheSynchronized") => {
            json!({ "kind": "contacts", "device": device })
        }
        ("device.mprisremote", _) => json!({ "kind": "media", "device": device }),
        // the phone's apps list (C1): the bytes come along, the sink writes
        // the cache and the page asks for the rows
        ("device.remotecommands", "commandsChanged") => {
            json!({ "kind": "apps", "device": device })
        }
        ("device.telephony", "callReceived") => json!({
            "kind": "call",
            "device": device,
            "event": text(0),
            "number": text(1),
            "name": text(2),
        }),
        ("device.share", "shareReceived") => {
            let url = text(0);
            let path = tools::path_of_url(url)
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            json!({ "kind": "received", "device": device, "url": url, "path": path })
        }
        ("device.sftp", "mounted") => {
            json!({ "kind": "storage", "device": device, "mounted": true })
        }
        ("device.sftp", "unmounted") => {
            json!({ "kind": "storage", "device": device, "mounted": false })
        }
        ("device.remotekeyboard", "remoteStateChanged") => json!({
            "kind": "keyboard",
            "device": device,
            "active": args.first().and_then(Json::as_bool).unwrap_or(false),
        }),
        // the phone typed a key it was sent (kdc::press asks it to say so).
        // Which key is not passed on: the page counts answers, nothing more.
        ("device.remotekeyboard", "keyPressReceived") => {
            json!({ "kind": "keyEcho", "device": device })
        }
        _ => return None,
    })
}

/// Read the daemon's signals for as long as the bus lasts; every one the
/// window cares about goes to `sink` as its event.
///
/// Only the daemon's own: the bus delivers a signal to whoever it is
/// addressed to, whatever its sender calls itself, so every one is checked
/// against the address that owns `org.kde.kdeconnect` right now. Anything
/// else on the session bus cannot make this window believe the phone said
/// something.
pub async fn pump(bus: Bus, mut sink: impl FnMut(Json)) -> zbus::Result<()> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(SERVICE)?
        .path_namespace(ROOT)?
        .build();
    let mut stream = zbus::MessageStream::for_match_rule(rule, bus.conn(), Some(512)).await?;
    let mut daemon = bus.owner().await;
    while let Some(msg) = stream.next().await {
        let Ok(msg) = msg else { continue };
        let header = msg.header();
        let from = header.sender().map(|s| s.as_str().to_string());
        if from.is_none() || from != daemon {
            // a restarted daemon has a new address: look once, then decide
            daemon = bus.owner().await;
            if from.is_none() || from != daemon {
                continue;
            }
        }
        let (Some(iface), Some(member), Some(path)) =
            (header.interface(), header.member(), header.path())
        else {
            continue;
        };
        let body = msg.body();
        let args: Vec<Json> = if body.signature().to_string().is_empty() {
            Vec::new()
        } else {
            match body.deserialize::<Structure<'_>>() {
                Ok(s) => s.fields().iter().map(to_json).collect(),
                Err(_) => continue,
            }
        };
        if let Some(ev) = event_for(iface.as_str(), member.as_str(), path.as_str(), &args) {
            sink(ev);
        }
    }
    Ok(())
}

/// After a change: write this phone's apps cache and entries when it is in
/// reach, and drop what is kept for phones that are no longer paired.
pub async fn housekeep(bus: &Bus, device: &str) {
    let Ok(list) = kdc::devices_of(bus).await else {
        return;
    };
    let paired: Vec<String> = list
        .iter()
        .filter(|d| d["paired"] == true)
        .filter_map(|d| d["id"].as_str().map(String::from))
        .collect();
    crate::apps::prune(&paired);
    if let Some(d) = list
        .iter()
        .find(|d| d["id"] == device && d["paired"] == true && d["reachable"] == true)
    {
        let name = d["name"].as_str().unwrap_or("");
        crate::apps::sync(bus, device, name, true).await;
    }
}

/// The daemon coming and going (first start, a crash, an update).
async fn owner(app: AppHandle, bus: Bus) -> zbus::Result<()> {
    let dbus = zbus::fdo::DBusProxy::new(bus.conn()).await?;
    let mut changes = dbus
        .receive_name_owner_changed_with_args(&[(0, SERVICE)])
        .await?;
    while let Some(change) = changes.next().await {
        let Ok(args) = change.args() else { continue };
        let running = args.new_owner().is_some();
        let _ = app.emit(EVENT, json!({ "kind": "daemon", "running": running }));
    }
    Ok(())
}

/// Listen for the app's whole life. A session bus that is not there yet is
/// tried again until it is (the page's own commands report it meanwhile):
/// a window that came up before its bus must not stay deaf all session.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let bus = loop {
            match app.state::<Link>().bus().await {
                Ok(b) => break b.clone(),
                Err(_) => tokio::time::sleep(std::time::Duration::from_secs(3)).await,
            }
        };
        let a = app.clone();
        let b = bus.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = owner(a, b).await {
                eprintln!("ewe-mobile: daemon watch ended: {e}");
            }
        });
        let keeper = bus.clone();
        let sink = |ev: Json| {
            // a file the daemon saved is one the page may open (kdc::open_file)
            let kind = ev["kind"].as_str().unwrap_or("");
            // the apps list changed, or a phone came, went or was unpaired:
            // the cache and the launcher entries follow (C2, C4)
            if kind == "apps" || kind == "devices" {
                let b = keeper.clone();
                let dev = ev["device"].as_str().unwrap_or("").to_string();
                tauri::async_runtime::spawn(async move { housekeep(&b, &dev).await });
            }
            if kind == "attachment" || kind == "received" {
                let path = ev["path"].as_str().unwrap_or("");
                // "received" also fires for a link the phone shared, with the
                // link as its text — and a link can name any file here. Only
                // one that has just been written was received.
                if kind == "received" && !tools::just_written(std::path::Path::new(path)) {
                    return;
                }
                if !path.is_empty() {
                    app.state::<Allowed>().allow(path.into());
                }
            }
            let _ = app.emit(EVENT, ev);
        };
        if let Err(e) = pump(bus, sink).await {
            eprintln!("ewe-mobile: signal stream ended: {e}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEV: &str = "/modules/kdeconnect/devices/abc123";

    /// The whole path a message takes — the phone, the daemon's signal, the
    /// match rule, the decoder — against the real phone. Asks for the five
    /// newest messages of one conversation (what opening it does); prints
    /// counts, never content.
    ///   cargo test live_signals -- --ignored --nocapture
    #[test]
    #[ignore = "needs a running kdeconnectd with a paired, reachable phone"]
    fn live_signals() {
        use std::sync::{Arc, Mutex};
        const CONV: &str = "org.kde.kdeconnect.device.conversations";
        tauri::async_runtime::block_on(async {
            let bus = Bus::connect().await.unwrap().passive();
            let devs = kdc::devices_of(&bus).await.unwrap();
            let id = devs
                .iter()
                .find(|d| d["paired"] == true && d["reachable"] == true)
                .expect("no paired, reachable device")["id"]
                .as_str()
                .unwrap()
                .to_string();
            let path = kdc::dev_path(&id).unwrap();
            let seen: Arc<Mutex<Vec<Json>>> = Arc::default();
            let sink_seen = seen.clone();
            let pump_bus = bus.clone();
            let task = tauri::async_runtime::spawn(async move {
                let _ = pump(pump_bus, move |ev| sink_seen.lock().unwrap().push(ev)).await;
            });
            // the match rule must be in place before the phone answers
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;

            // the list: a daemon that has it cached answers "loaded" per
            // thread, a fresh one sends each thread's newest message
            bus.call(&path, CONV, "requestAllConversationThreads", &())
                .await
                .unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let cached = bus
                .call1(&path, CONV, "activeConversations", &())
                .await
                .unwrap();
            let thread = cached
                .as_array()
                .and_then(|a| a.iter().filter_map(kdc::message_json).next())
                .expect("the phone has no conversations")["thread"]
                .as_i64()
                .unwrap();

            // one conversation: its messages arrive as signals
            bus.call(&path, CONV, "requestConversation", &(thread, 0i32, 5i32))
                .await
                .unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            task.abort();

            let seen = seen.lock().unwrap();
            let mut kinds = std::collections::BTreeMap::<String, usize>::new();
            for e in seen.iter() {
                *kinds
                    .entry(e["kind"].as_str().unwrap_or("?").to_string())
                    .or_default() += 1;
            }
            println!("events: {kinds:?}");
            let of_thread: Vec<&Json> = seen
                .iter()
                .filter(|e| e["kind"] == "message" && e["msg"]["thread"].as_i64() == Some(thread))
                .collect();
            assert!(
                !of_thread.is_empty(),
                "no message of the asked conversation arrived"
            );
            for m in &of_thread {
                assert_eq!(m["device"], id.as_str());
                assert!(m["msg"]["date"].as_i64().unwrap() > 1_000_000_000_000);
                assert!(!m["msg"]["addresses"].as_array().unwrap().is_empty());
            }
            assert!(
                seen.iter()
                    .any(|e| e["kind"] == "threadLoaded" && e["thread"].as_i64() == Some(thread)),
                "the daemon never said the conversation was loaded"
            );
        });
    }

    /// A stand-in for kdeconnectd, or an impostor: `<role> <charge>`. The
    /// daemon owns the name; the impostor has none and sends its signal to
    /// everyone and straight at every client on the bus.
    const ACTOR: &str = r#"
import sys, dbus, dbus.service, dbus.lowlevel
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib
DBusGMainLoop(set_as_default=True)
bus = dbus.SessionBus()
role, charge = sys.argv[1], int(sys.argv[2])
if role == "daemon":
    name = dbus.service.BusName("org.kde.kdeconnect", bus)
PATH = "/modules/kdeconnect/devices/abc123/battery"
def send(dest=None):
    m = dbus.lowlevel.SignalMessage(PATH, "org.kde.kdeconnect.device.battery", "refreshed")
    m.append(True, dbus.Int32(charge), signature="bi")
    if dest:
        m.set_destination(dest)
    bus.send_message(m)
def tick():
    send()
    if role == "impostor":
        for n in bus.list_names():
            if n.startswith(":") and n != bus.get_unique_name():
                send(n)
    return True
GLib.timeout_add(150, tick)
GLib.MainLoop().run()
"#;

    /// Only the daemon's signals pass — run by `tests/origin.sh`, which puts
    /// it on a private bus. Refuses to run anywhere else: the stand-in would
    /// take the real daemon's name.
    #[test]
    #[ignore = "run through tests/origin.sh (a private session bus)"]
    fn private_bus_origin() {
        use std::process::{Command, Stdio};
        use std::sync::{Arc, Mutex};
        assert_eq!(
            std::env::var("EWE_MOBILE_TEST_BUS").as_deref(),
            Ok("private"),
            "run tests/origin.sh — this must never run on the real session bus"
        );
        let actor = |role: &str, charge: &str| {
            Command::new("python3")
                .args(["-c", ACTOR, role, charge])
                .stdin(Stdio::null())
                .spawn()
                .expect("python3 with dbus and gi")
        };
        tauri::async_runtime::block_on(async {
            let bus = Bus::connect().await.unwrap().passive();
            assert!(
                bus.owner().await.is_none(),
                "something already owns org.kde.kdeconnect here"
            );
            let mut daemon = actor("daemon", "81");
            let mut impostor = actor("impostor", "13");
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

            let seen: Arc<Mutex<Vec<i64>>> = Arc::default();
            let sink_seen = seen.clone();
            let task = tauri::async_runtime::spawn(async move {
                let _ = pump(bus, move |ev| {
                    if ev["kind"] == "battery" {
                        sink_seen
                            .lock()
                            .unwrap()
                            .push(ev["charge"].as_i64().unwrap_or(-1));
                    }
                })
                .await;
            });
            tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
            task.abort();
            let _ = daemon.kill();
            let _ = impostor.kill();
            let _ = daemon.wait();
            let _ = impostor.wait();

            let seen = seen.lock().unwrap();
            let real = seen.iter().filter(|c| **c == 81).count();
            let fake = seen.iter().filter(|c| **c == 13).count();
            println!("from the daemon: {real}; from the impostor: {fake}");
            assert!(real >= 3, "the daemon's own signals did not arrive");
            assert_eq!(
                fake, 0,
                "a signal from someone else was taken for the daemon's"
            );
        });
    }

    #[test]
    fn the_device_comes_from_the_path() {
        assert_eq!(device_of(DEV), "abc123");
        assert_eq!(device_of(&format!("{DEV}/notifications/7")), "abc123");
        assert_eq!(device_of("/modules/kdeconnect"), "");
        assert_eq!(device_of("/org/kde/other"), "");
    }

    #[test]
    fn battery_and_signal_carry_their_values() {
        let e = event_for(
            "org.kde.kdeconnect.device.battery",
            "refreshed",
            &format!("{DEV}/battery"),
            &[json!(true), json!(81)],
        )
        .unwrap();
        assert_eq!(
            e,
            json!({ "kind": "battery", "device": "abc123", "charging": true, "charge": 81 })
        );
        let e = event_for(
            "org.kde.kdeconnect.device.connectivity_report",
            "refreshed",
            &format!("{DEV}/connectivity_report"),
            &[json!("LTE"), json!(3)],
        )
        .unwrap();
        assert_eq!(e["type"], "LTE");
        assert_eq!(e["strength"], 3);
    }

    #[test]
    fn a_message_signal_is_decoded() {
        let wire = json!([1, "hi", [["+1"]], 10, 1, 1, 4, 2, -1, []]);
        let e = event_for(
            "org.kde.kdeconnect.device.conversations",
            "conversationUpdated",
            DEV,
            &[wire],
        )
        .unwrap();
        assert_eq!(e["kind"], "message");
        assert_eq!(e["msg"]["thread"], 4);
        // a message that cannot be read is dropped, not passed on broken
        assert!(event_for(
            "org.kde.kdeconnect.device.conversations",
            "conversationUpdated",
            DEV,
            &[json!("?")]
        )
        .is_none());
    }

    #[test]
    fn state_changes_only_say_what_changed() {
        let e = event_for(
            "org.kde.kdeconnect.device",
            "reachableChanged",
            DEV,
            &[json!(false)],
        );
        assert_eq!(e.unwrap(), json!({ "kind": "devices" }));
        let e = event_for(
            "org.kde.kdeconnect.device.notifications",
            "notificationPosted",
            &format!("{DEV}/notifications"),
            &[json!("12")],
        );
        assert_eq!(
            e.unwrap(),
            json!({ "kind": "notifications", "device": "abc123" })
        );
        let e = event_for(
            "org.kde.kdeconnect.device.share",
            "shareReceived",
            &format!("{DEV}/share"),
            &[json!("file:///home/you/Downloads/a%20b.jpg")],
        )
        .unwrap();
        assert_eq!(e["path"], "/home/you/Downloads/a b.jpg");
        let e = event_for(
            "org.kde.kdeconnect.device.remotecommands",
            "commandsChanged",
            &format!("{DEV}/remotecommands"),
            &[json!([123, 125])],
        );
        assert_eq!(e.unwrap(), json!({ "kind": "apps", "device": "abc123" }));
        assert!(event_for(
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            DEV,
            &[]
        )
        .is_none());
        assert!(event_for(
            "org.kde.kdeconnect.device.clipboard",
            "autoShareDisabledChanged",
            DEV,
            &[]
        )
        .is_none());
    }

    #[test]
    fn the_phones_keyboard_says_two_things() {
        let path = format!("{DEV}/remotekeyboard");
        let e = event_for(
            "org.kde.kdeconnect.device.remotekeyboard",
            "remoteStateChanged",
            &path,
            &[json!(true)],
        );
        assert_eq!(
            e.unwrap(),
            json!({ "kind": "keyboard", "device": "abc123", "active": true })
        );
        // a key it typed: that it did, never which
        let e = event_for(
            "org.kde.kdeconnect.device.remotekeyboard",
            "keyPressReceived",
            &path,
            &[
                json!("a"),
                json!(0),
                json!(false),
                json!(false),
                json!(false),
                json!(false),
            ],
        );
        assert_eq!(e.unwrap(), json!({ "kind": "keyEcho", "device": "abc123" }));
    }
}
