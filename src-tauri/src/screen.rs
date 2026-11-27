//! The Screen and Apps pages' commands, and the headless verbs of C3
//! (`--apps`, `--open-app`, `--screen`, `--adb-status`): what the window
//! asks over `invoke`, and what the dock popup and the launcher entries ask
//! on the command line. The same functions underneath (`apps.rs`, `adb.rs`).
//!
//! A headless verb opens no window, never starts kdeconnectd (a passive bus
//! handle), spawns scrcpy detached and returns at once; it prints one JSON
//! line and exits 0 whatever happened (house rule 5).

use std::sync::Mutex;

use serde_json::{json, Value as Json};
use tauri::State;

use crate::adb::{self, ScreenOpts};
use crate::apps;
use crate::bus::Bus;
use crate::kdc::{self, Link};

/// A scrcpy window this process started: the mirrored screen or one app.
struct Win {
    class: String,
    title: String,
    package: Option<String>,
    child: std::process::Child,
}

/// The scrcpy windows this app opened, so the page can show which are up
/// and stop them. A window outlives the app; this list does not.
#[derive(Default)]
pub struct Windows(Mutex<Vec<Win>>);

impl Windows {
    fn push(
        &self,
        class: String,
        title: String,
        package: Option<String>,
        child: std::process::Child,
    ) {
        if let Ok(mut w) = self.0.lock() {
            // the same app opened again: the old window is no longer listed,
            // but its process is still ours to reap (else it stays a zombie)
            let (gone, keep): (Vec<Win>, Vec<Win>) = std::mem::take(&mut *w)
                .into_iter()
                .partition(|x| x.class == class);
            *w = keep;
            for mut old in gone {
                std::thread::spawn(move || {
                    let _ = old.child.wait();
                });
            }
            w.push(Win {
                class,
                title,
                package,
                child,
            });
        }
    }
    /// Reap what ended; what is still up, as JSON. `died` carries the last
    /// complaint of a window that ended since the previous look.
    fn state(&self) -> Json {
        let mut died = Vec::new();
        let mut up = Vec::new();
        if let Ok(mut w) = self.0.lock() {
            w.retain_mut(|x| match x.child.try_wait() {
                Ok(Some(status)) => {
                    let words = adb::last_words(&x.class);
                    died.push(json!({ "class": x.class, "title": x.title, "package": x.package, "ok": status.success(), "words": words }));
                    false
                }
                Ok(None) => {
                    up.push(json!({ "class": x.class, "title": x.title, "package": x.package }));
                    true
                }
                Err(_) => false,
            });
        }
        json!({ "windows": up, "died": died })
    }
    fn stop(&self, class: &str) -> bool {
        let Ok(mut w) = self.0.lock() else {
            return false;
        };
        let mut found = false;
        w.retain_mut(|x| {
            if x.class == class {
                found = true;
                let _ = x.child.kill();
                let _ = x.child.wait();
                false
            } else {
                true
            }
        });
        found
    }
    pub fn any_running(&self) -> bool {
        self.0
            .lock()
            .map(|mut w| w.iter_mut().any(|x| matches!(x.child.try_wait(), Ok(None))))
            .unwrap_or(false)
    }
}

/// The phone this is about: the one named, else the paired one in reach,
/// else the paired one. `(id, name, reachable)`.
async fn pick_device(
    bus: &Bus,
    wanted: Option<&str>,
) -> Result<(String, String, bool), (String, String)> {
    if !bus.daemon_running().await {
        return Err((
            "no-device".into(),
            "The phone link (KDE Connect) isn’t running.".into(),
        ));
    }
    let list = kdc::devices_of(bus)
        .await
        .map_err(|e| ("no-device".to_string(), e))?;
    let pick = match wanted {
        Some(id) => list.iter().find(|d| d["id"] == id && d["paired"] == true),
        None => list
            .iter()
            .find(|d| d["paired"] == true && d["reachable"] == true)
            .or_else(|| list.iter().find(|d| d["paired"] == true)),
    };
    match pick {
        Some(d) => Ok((
            d["id"].as_str().unwrap_or("").to_string(),
            d["name"].as_str().unwrap_or("").to_string(),
            d["reachable"].as_bool().unwrap_or(false),
        )),
        None => Err((
            "no-device".into(),
            "No phone is paired with this computer.".into(),
        )),
    }
}

fn err(code: &str, msg: &str) -> Json {
    json!({ "ok": false, "error": code, "message": msg })
}

fn cache_rows(rows: &[apps::Cached]) -> Json {
    serde_json::to_value(rows).unwrap_or(Json::Array(vec![]))
}

// ── the Apps page ───────────────────────────────────────────────────────────

/// The phone's apps: live from the phone when it is in reach and runs ewe
/// Mobile (writing C2 and C4 on the way), else from the cache. Icons as
/// `data:` URLs for the page.
#[tauri::command]
pub async fn apps(link: State<'_, Link>, device: String) -> Result<Json, String> {
    let bus = link.bus().await?;
    let (id, name, reachable) = pick_device(bus, Some(&device)).await.map_err(|(_, m)| m)?;
    let (source, rows, updated) = apps::sync(bus, &id, &name, reachable).await;
    Ok(json!({
        "source": source,
        "updated": updated,
        "apps": rows.iter().map(apps::row_json).collect::<Vec<_>>(),
    }))
}

/// Ask the phone for its list again.
#[tauri::command]
pub async fn apps_refresh(link: State<'_, Link>, device: String) -> Result<(), String> {
    apps::trigger(link.bus().await?, &device, "ewe:refresh").await
}

/// The fallback list, from `scrcpy --list-apps` (12.5 s a call, so it is
/// kept in the device's cache folder for a day unless `refresh`).
#[tauri::command]
pub async fn apps_scrcpy(
    link: State<'_, Link>,
    device: String,
    refresh: bool,
) -> Result<Json, String> {
    let bus = link.bus().await?;
    let (id, name, _) = pick_device(bus, Some(&device)).await.map_err(|(_, m)| m)?;
    let file = apps::device_dir(&apps::cache_base(), &id)
        .ok_or("bad device id")?
        .join("scrcpy-apps.json");
    if !refresh {
        if let Ok(text) = std::fs::read_to_string(&file) {
            if let Ok(v) = serde_json::from_str::<Json>(&text) {
                let age = apps::now_ms().saturating_sub(v["updated"].as_u64().unwrap_or(0));
                if age < 24 * 3600 * 1000 && v["apps"].is_array() {
                    return Ok(v);
                }
            }
        }
    }
    let serial = adb::serial_for(&name).await.map_err(|(_, m)| m)?;
    let list = adb::list_apps(&serial).await?;
    let v = json!({
        "updated": apps::now_ms(),
        "apps": list.iter().map(|(n, p, s)| json!({ "package": p, "name": n, "sys": s, "icon": "" })).collect::<Vec<_>>(),
    });
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&file, v.to_string());
    Ok(v)
}

/// Open an app: `here` (its own window, needs adb), `phone` (on the phone's
/// screen, through C1), or `auto` (here when adb is connected, else phone).
/// Answers where it went.
#[tauri::command]
pub async fn app_open(
    link: State<'_, Link>,
    windows: State<'_, Windows>,
    server: State<'_, adb::Server>,
    device: String,
    package: String,
    name: String,
    r#where: String,
) -> Result<String, String> {
    let bus = link.bus().await?;
    if r#where != "phone" {
        server.ensure().await;
    }
    match open_app(bus, Some(&device), &package, &r#where).await {
        Ok((place, child)) => {
            if let Some(child) = child {
                let title = if name.trim().is_empty() {
                    package.clone()
                } else {
                    name
                };
                windows.push(apps::app_class(&package), title, Some(package), child);
            }
            Ok(place)
        }
        Err((_, m)) => Err(m),
    }
}

/// The shared heart of `app_open` and `--open-app`.
async fn open_app(
    bus: &Bus,
    device: Option<&str>,
    package: &str,
    place: &str,
) -> Result<(String, Option<std::process::Child>), (String, String)> {
    if !apps::package_ok(package) {
        return Err(("unknown-app".into(), "That isn’t a package name.".into()));
    }
    let (id, name, reachable) = pick_device(bus, device).await?;
    let cached = apps::read_cache(&apps::cache_base(), &id);
    let title = cached
        .as_ref()
        .and_then(|(rows, _, _)| {
            rows.iter()
                .find(|r| r.package == package)
                .map(|r| r.name.clone())
        })
        .unwrap_or_else(|| package.to_string());
    let here = match place {
        "here" => true,
        "phone" => false,
        _ => adb::serial_for(&name).await.is_ok(),
    };
    if here {
        let serial = adb::serial_for(&name).await?;
        let dark = adb::wake(&serial).await;
        let child = adb::spawn(
            &apps::app_class(package),
            &adb::app_args(&serial, package, &title, dark),
        )
        .map_err(|e| {
            (
                "scrcpy-failed".to_string(),
                format!("scrcpy didn’t start: {e}"),
            )
        })?;
        return Ok(("here".into(), Some(child)));
    }
    if !reachable {
        return Err(("away".into(), format!("{name} isn’t in reach.")));
    }
    let live = apps::apps_live(bus, &id)
        .await
        .map_err(|e| ("away".to_string(), e))?;
    let Some(list) = live else {
        return Err(("unknown-app".into(), format!("{name} runs the stock KDE Connect app, which can’t open apps. Install ewe Mobile on it.")));
    };
    if !list.iter().any(|a| a.package == package) {
        return Err((
            "unknown-app".into(),
            "The phone doesn’t have that app.".into(),
        ));
    }
    apps::trigger(bus, &id, package)
        .await
        .map_err(|e| ("away".to_string(), e))?;
    Ok(("phone".into(), None))
}

#[tauri::command]
pub async fn launcher_setting() -> bool {
    apps::launcher_setting().await
}

/// Flip "Show the phone's apps in the launcher" and rewrite every entry.
#[tauri::command]
pub async fn set_launcher_setting(link: State<'_, Link>, on: bool) -> Result<(), String> {
    apps::set_launcher_setting(on).await?;
    let bus = link.bus().await?;
    if let Ok(list) = kdc::devices_of(bus).await {
        for d in list.iter().filter(|d| d["paired"] == true) {
            let id = d["id"].as_str().unwrap_or("");
            let name = d["name"].as_str().unwrap_or("");
            if let Some((rows, _, _)) = apps::read_cache(&apps::cache_base(), id) {
                let _ = apps::write_entries(&apps::entries_dir(), id, name, &rows, on);
            }
        }
    }
    Ok(())
}

// ── the Screen page ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn adb_status(
    link: State<'_, Link>,
    server: State<'_, adb::Server>,
    device: String,
) -> Result<Json, String> {
    let bus = link.bus().await?;
    server.ensure().await;
    let name = pick_device(bus, Some(&device))
        .await
        .map(|(_, n, _)| n)
        .unwrap_or_default();
    Ok(adb::status(&name).await)
}

#[tauri::command]
pub async fn adb_services() -> Result<Vec<adb::Service>, String> {
    adb::services().await
}

#[tauri::command]
pub async fn adb_pair(host: String, port: u16, code: String) -> Result<String, String> {
    adb::pair(&host, port, &code).await
}

#[tauri::command]
pub async fn adb_connect(host: String, port: u16) -> Result<(), String> {
    adb::connect(&host, port).await
}

/// Komble, on the package that is missing. False: no Komble either.
#[tauri::command]
pub async fn tools_offer() -> bool {
    adb::offer_tools(&adb::tools())
}

#[tauri::command]
pub async fn screen_start(
    link: State<'_, Link>,
    windows: State<'_, Windows>,
    device: String,
    opts: ScreenOpts,
) -> Result<(), String> {
    let bus = link.bus().await?;
    let (_, name, _) = pick_device(bus, Some(&device)).await.map_err(|(_, m)| m)?;
    let serial = adb::serial_for(&name).await.map_err(|(_, m)| m)?;
    let child = adb::spawn(adb::SCREEN_CLASS, &adb::screen_args(&serial, &name, &opts))
        .map_err(|e| format!("scrcpy didn’t start: {e}"))?;
    windows.push(adb::SCREEN_CLASS.into(), name, None, child);
    Ok(())
}

/// Stop a window this app opened (the screen, or one app by its class).
#[tauri::command]
pub async fn window_stop(windows: State<'_, Windows>, class: String) -> Result<bool, String> {
    Ok(windows.stop(&class))
}

/// Which scrcpy windows of ours are up, and which just ended (with words).
#[tauri::command]
pub async fn windows_state(windows: State<'_, Windows>) -> Result<Json, String> {
    Ok(windows.state())
}

// ── C3: the command line ────────────────────────────────────────────────────

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == flag {
            return it.next().cloned();
        }
        if let Some(v) = a.strip_prefix(&format!("{flag}=")) {
            return Some(v.to_string());
        }
    }
    None
}

/// Is this a headless verb? Then its one JSON line.
pub fn headless(args: &[String]) -> Option<Json> {
    let has = |f: &str| {
        args.iter()
            .any(|a| a == f || a.starts_with(&format!("{f}=")))
    };
    if !(has("--apps") || has("--open-app") || has("--screen") || has("--adb-status")) {
        return None;
    }
    Some(tauri::async_runtime::block_on(run(args)))
}

async fn run(args: &[String]) -> Json {
    let has = |f: &str| {
        args.iter()
            .any(|a| a == f || a.starts_with(&format!("{f}=")))
    };
    let device = flag_value(args, "--device");
    let bus = match Bus::connect().await {
        Ok(b) => b.passive(),
        Err(e) => return err("no-device", &e),
    };
    if has("--adb-status") {
        let name = pick_device(&bus, device.as_deref())
            .await
            .map(|(_, n, _)| n)
            .unwrap_or_default();
        return adb::status(&name).await;
    }
    if has("--apps") {
        let (id, name, reachable) = match pick_device(&bus, device.as_deref()).await {
            Ok(d) => d,
            Err((c, m)) => return err(&c, &m),
        };
        let (source, rows, updated) = apps::sync(&bus, &id, &name, reachable).await;
        let adb_ok = adb::serial_for(&name).await.is_ok();
        return json!({
            "ok": true,
            "device": id,
            "deviceName": name,
            "reachable": reachable,
            "adb": adb_ok,
            "source": source,
            "updated": updated,
            "apps": cache_rows(&rows),
        });
    }
    if has("--screen") {
        let (_, name, _) = match pick_device(&bus, device.as_deref()).await {
            Ok(d) => d,
            Err((c, m)) => return err(&c, &m),
        };
        let serial = match adb::serial_for(&name).await {
            Ok(s) => s,
            Err((c, m)) => return err(&c, &m),
        };
        return match adb::spawn(
            adb::SCREEN_CLASS,
            &adb::screen_args(&serial, &name, &ScreenOpts::default()),
        ) {
            Ok(child) => {
                json!({ "ok": true, "class": adb::SCREEN_CLASS, "title": name, "pid": child.id() })
            }
            Err(e) => err("scrcpy-failed", &format!("scrcpy didn’t start: {e}")),
        };
    }
    // --open-app
    let Some(package) = flag_value(args, "--open-app") else {
        return err("unknown-app", "--open-app needs a package name.");
    };
    let place = if has("--here") {
        "here"
    } else if has("--phone") {
        "phone"
    } else {
        "auto"
    };
    match open_app(&bus, device.as_deref(), &package, place).await {
        Ok((where_, child)) => json!({
            "ok": true,
            "package": package,
            "where": where_,
            "class": child.as_ref().map(|_| apps::app_class(&package)),
            "pid": child.map(|c| c.id()),
        }),
        Err((c, m)) => err(&c, &m),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn flags_are_read_both_ways() {
        assert_eq!(
            flag_value(&a(&["--open-app", "com.x"]), "--open-app").as_deref(),
            Some("com.x")
        );
        assert_eq!(
            flag_value(&a(&["--open-app=com.x", "--here"]), "--open-app").as_deref(),
            Some("com.x")
        );
        assert_eq!(
            flag_value(&a(&["--device", "ab12"]), "--device").as_deref(),
            Some("ab12")
        );
        assert_eq!(flag_value(&a(&["--apps"]), "--device"), None);
    }

    #[test]
    fn only_the_four_verbs_are_headless() {
        // on a private bus with no daemon every verb answers no-device, exit 0
        for args in [&["--page", "apps"][..], &["--version"], &[]] {
            assert!(headless(&a(args)).is_none());
        }
    }
}
