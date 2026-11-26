//! ewe Mobile — your phone on your desktop.
//!
//! A window over KDE Connect's daemon: the daemon keeps the link to the
//! phone, this app is everything you see of it. Nothing stays behind when
//! the window closes — the bar's phone glyph and the Quick settings page are
//! the ewe.phone plugin's, in the shell (a Tauri window cannot be a layer
//! surface).
//!
//!   ewe-mobile                 open the window (a second launch focuses it)
//!   ewe-mobile --page <id>     … on a page: phone messages contacts
//!                              notifications media files screen apps remote devices
//!   ewe-mobile --status        one JSON line: the daemon and its devices
//!   ewe-mobile --version       one JSON line
//!   ewe-mobile --apps          the phone's apps (C2 rows), one JSON line
//!   ewe-mobile --open-app <package> [--here|--phone]
//!   ewe-mobile --screen        the phone's screen in a window (scrcpy)
//!   ewe-mobile --adb-status    the debugging link
//!
//! Every verb prints JSON and exits 0, like every ewe tool; the four of
//! `docs/ECOSYSTEM.md` C3 open no window and never start kdeconnectd.

mod adb;
mod apps;
mod bus;
mod contacts;
mod events;
mod kdc;
mod screen;
mod tools;

use std::sync::Mutex;

use tauri::{
    AppHandle, DragDropEvent, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder,
    WindowEvent,
};

const MAIN: &str = "main";

/// The pages `--page` may name. Public: the shell plugin and other apps open
/// them, so an id never changes with its label.
const PAGES: [&str; 10] = [
    "phone",
    "messages",
    "contacts",
    "notifications",
    "media",
    "files",
    "screen",
    "apps",
    "remote",
    "devices",
];

/// The page a launch asked for, until the window takes it.
struct Page(Mutex<Option<String>>);

fn parse_page(args: &[String]) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let value = if a == "--page" {
            it.next().map(String::as_str)
        } else {
            a.strip_prefix("--page=")
        };
        if let Some(v) = value {
            return PAGES.contains(&v).then(|| v.to_string());
        }
    }
    None
}

#[tauri::command]
fn take_page(page: State<'_, Page>) -> Option<String> {
    page.0.lock().ok().and_then(|mut p| p.take())
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        return;
    }
    // undecorated: Hyprland tiles it and draws the border
    let built = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::default())
        .title("ewe Mobile")
        .inner_size(1040.0, 700.0)
        .min_inner_size(420.0, 520.0)
        .center()
        .decorations(false)
        .build();
    if let Err(e) = built {
        eprintln!("ewe-mobile: could not open the window: {e}");
    }
}

pub fn run() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{}", tools::version_json());
        return;
    }
    if args.iter().any(|a| a == "--status") {
        println!("{}", tauri::async_runtime::block_on(kdc::status_line()));
        return;
    }
    if let Some(line) = screen::headless(&args) {
        println!("{line}");
        return;
    }
    let page = parse_page(&args);

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // a second launch (the launcher, the bar's phone glyph) brings
            // the window forward, on the page it named
            let rest: Vec<String> = argv.into_iter().skip(1).collect();
            if let Some(p) = parse_page(&rest) {
                if let Ok(mut slot) = app.state::<Page>().0.lock() {
                    *slot = Some(p);
                }
                let _ = app.emit("page", ());
            }
            show_main(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .manage(kdc::Link::default())
        .manage(kdc::Allowed::default())
        .manage(kdc::Offered::default())
        .manage(Page(Mutex::new(page)))
        .manage(screen::Windows::default())
        .manage(adb::Server::default())
        // files dropped on the window are the ones `share_files` may send
        .on_window_event(|window, event| {
            if let WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event {
                window.state::<kdc::Offered>().offer(paths);
            }
        })
        .setup(|app| {
            events::start(app.handle().clone());
            show_main(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            // A2: an adb server this app started goes when the window does —
            // unless a scrcpy window of ours still needs it
            if let WindowEvent::Destroyed = event {
                let app = window.app_handle().clone();
                if app.state::<adb::Server>().started_here()
                    && !app.state::<screen::Windows>().any_running()
                {
                    tools::spawn_reaped("adb", &["kill-server"]);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            take_page,
            tools::de_prefs,
            tools::theme_tokens,
            kdc::status,
            kdc::daemon_start,
            kdc::devices,
            kdc::rescan,
            kdc::set_name,
            kdc::pair,
            kdc::plugins,
            kdc::set_plugin,
            kdc::notifications,
            kdc::notification_dismiss,
            kdc::notification_reply,
            kdc::conversations,
            kdc::conversation_load,
            kdc::send_message,
            kdc::attachment_fetch,
            kdc::contacts,
            kdc::contact_photo,
            kdc::contacts_sync,
            kdc::media,
            kdc::media_action,
            kdc::media_set,
            kdc::share_files,
            kdc::share_pick,
            kdc::share_text,
            kdc::dial,
            kdc::act,
            kdc::storage_state,
            kdc::storage_open,
            kdc::storage_close,
            kdc::key_send,
            kdc::text_send,
            kdc::keyboard_state,
            kdc::pointer_move,
            kdc::pointer_do,
            kdc::open_file,
            kdc::open_link,
            screen::apps,
            screen::apps_refresh,
            screen::apps_scrcpy,
            screen::app_open,
            screen::launcher_setting,
            screen::set_launcher_setting,
            screen::adb_status,
            screen::adb_services,
            screen::adb_pair,
            screen::adb_connect,
            screen::tools_offer,
            screen::screen_start,
            screen::window_stop,
            screen::windows_state,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ewe Mobile");
}

#[cfg(test)]
mod tests {
    use super::parse_page;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_page_flag() {
        assert_eq!(
            parse_page(&args(&["--page", "messages"])).as_deref(),
            Some("messages")
        );
        assert_eq!(
            parse_page(&args(&["--page=files"])).as_deref(),
            Some("files")
        );
        assert_eq!(
            parse_page(&args(&["--page", "screen"])).as_deref(),
            Some("screen")
        );
        assert_eq!(parse_page(&args(&["--page=apps"])).as_deref(), Some("apps"));
        assert_eq!(parse_page(&args(&["--page", "nowhere"])), None);
        assert_eq!(parse_page(&args(&["--page"])), None);
        assert_eq!(parse_page(&args(&[])), None);
    }
}
