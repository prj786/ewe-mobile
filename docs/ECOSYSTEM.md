# ewe Mobile — one ecosystem (build plan, 2026-11-26 → weekend)

The owner, 2026-11-26: see the phone's screen on the PC and open the phone's
apps in ewe; an Android app with ewe's own look (minimal, modern, every
feature KDE Connect has); phone notifications that look like normal ewe
notifications — no initials — marked so you know they came from the phone;
no KDE Connect window anywhere, only ewe Mobile; a dock button with the
phone's files and apps. "Imagine it as one ecosystem."

This file is the plan **and the contracts** every part is built against.
It is owned by the orchestrating session; agents read it, never edit it.
RFC-012 (`ewe/docs/RFC-012-ewe-mobile-android.md`) stays the why; this is
the how and who.

## Decisions taken today (owner)

| | Decision |
|---|---|
| Phone notifications on the PC | the phone app's own icon and name; card outlined in the **accent colour** with a **phone chip** (glyph + phone name) in the header |
| Stock KDE Connect on the phone | uninstalled over adb **once ewe's build works**; its old entry unpaired on the PC |
| KDE Connect on the PC | `kdeconnectd` stays as the invisible engine; every KDE Connect window and launcher entry is hidden |
| Phone apps on the PC | dock popup + ewe Mobile's Apps page + **the launcher** (Super+D), marked as phone apps; an app opened here gets its real icon in the dock and Overview |
| Git | feature branches with local commits; nothing pushed, `main` untouched until the owner says |

## The picture

```
 phone: ewe Mobile for Android (fork of kdeconnect-android)
   ├─ the ewe face (Compose, ewe tokens)                       ── B2
   ├─ EweAppsPlugin: launchable apps + icons, "open <pkg>"      ── B3
   └─ notifications carry the APP icon, never the large icon   ── B3
        │ KDE Connect protocol (TLS, paired)
 PC: kdeconnectd (stock, headless) ── session D-Bus ──┬─ ewe Mobile window (Tauri)   Screen, Apps, …
                                                      ├─ ewe.phone plugin (shell)    dock button + popup, Mobile page, bar glyph
                                                      └─ ewe shell notifications     phone-styled cards, inline reply
        adb over Wi-Fi (Wireless debugging) ── scrcpy ── phone screen / one app per window
```

## Contracts

### C1 — the apps list (phone → PC), through `remotecommands`

The phone announces `kdeconnect.runcommand` (out) and
`kdeconnect.runcommand.request` (in) — proven by the B3 spike. It sends:

```json
{ "commandList": "<JSON string, below>", "canAddCommand": false }
```

`commandList` parses to one object:

```json
{
  "ewe": { "name": "ewe Mobile", "command": "", "v": 1,
           "count": 93, "app": "<versionName>", "sdk": 37 },
  "com.whatsapp": { "name": "WhatsApp", "command": "com.whatsapp",
                    "icon": "<base64 PNG>", "sys": false },
  "…": { }
}
```

- One entry per **launchable** app (`ACTION_MAIN` + `CATEGORY_LAUNCHER`),
  key = package = `command`; ewe Mobile itself is left out. `sys` = system app.
- `icon`: base64 PNG, square, the app's **launcher** icon, 96 px unless that
  pushes the whole list past ~700 KB (then 72 px). Never WebP (the shell
  cannot read it).
- The `"ewe"` key marks the list as ours; `v` versions it. A list without it
  is someone else's command list: ignore it.
- Sent on connect (kdeconnectd asks with `requestCommandList`), on
  `ewe:refresh`, and when a package is added, removed or changed (debounced).
- PC → phone: `triggerCommand(key)` arrives as `{"key": key}`:
  - `<package>` → start that app on the phone's screen (background start is
    allowed while the accessibility service is on; otherwise a notification
    "Open <App>" to tap).
  - `ewe:refresh` → send the list again.
  - every other `ewe:` key is reserved; unknown keys are ignored.
- On the PC, a device **is ewe Mobile** when
  `/modules/kdeconnect/devices/<id>/remotecommands` exists and its `commands`
  (bytes, UTF-8 JSON) parse with an `"ewe"` key whose `v` ≥ 1. Keys, names and
  icons are strings someone else wrote: validate them (package =
  `[A-Za-z0-9._]{1,255}`, name ≤ 200 chars, icon decodes as PNG ≤ 256 KB).

### C2 — the apps cache on the PC (written by every reader, same format)

```
$XDG_CACHE_HOME/ewe-mobile/<deviceId>/apps.json
$XDG_CACHE_HOME/ewe-mobile/<deviceId>/icons/<package>.png
```

```json
{ "v": 1, "device": "<id>", "deviceName": "Pixel 9 Pro XL", "updated": 1764170000000,
  "apps": [ { "package": "com.whatsapp", "name": "WhatsApp", "sys": false,
              "icon": "/home/u/.cache/ewe-mobile/<id>/icons/com.whatsapp.png" } ] }
```

- Sorted by name (case-insensitive). Written atomically (temp file + rename);
  icons rewritten only when their bytes change. A package that left the list
  loses its icon file.
- Writers: ewe Mobile (Rust) and the ewe.phone bridge (Python) — whichever
  sees `commandsChanged` or starts first. Readers: the dock popup, the
  launcher entries (C4), ewe Mobile's Apps page. The cache is what makes the
  dock and launcher work while the phone is away (dimmed, "phone not in reach").

### C3 — `ewe-mobile` command line (JSON, exit 0 — house rule 5)

Existing: `--page <id>`, `--status`, `--version`. New:

```
ewe-mobile --page screen|apps                  the new pages (public page ids)
ewe-mobile --apps [--device <id>]              {"ok":true,"device":…,"reachable":…,"adb":…,"apps":[C2 entries]}
ewe-mobile --open-app <package> [--here|--phone] [--device <id>]
                                               --here: its own window on this PC (scrcpy virtual display; needs adb)
                                               --phone: start it on the phone's screen (C1 trigger; no adb)
                                               neither: --here when adb is connected, else --phone
ewe-mobile --screen [--device <id>]            mirror the phone's screen in a window (scrcpy)
ewe-mobile --adb-status                        {"ok":true,"tools":…,"paired":…,"connected":…,"serial":…}
```

Headless verbs never open the main window and never start `kdeconnectd`;
they spawn scrcpy detached and return at once. Failures are
`{"ok":false,"error":"<code>","message":"<words>"}` with codes
`no-device`, `away`, `no-adb-tools`, `adb-not-paired`, `adb-not-connected`,
`unknown-app`, `scrcpy-failed`.

### C4 — phone apps as windows and launcher entries

- ewe Mobile starts scrcpy with `SDL_APP_ID` (scrcpy 5.0.1 is SDL3) so every
  window has a stable class:
  - the mirrored screen: `ewe-mobile-screen`, title `<phone name>`
  - one app: `ewe-mobile-app-<package>` (dots kept), title `<App name>`
- ewe Mobile writes one desktop entry per phone app (from C2):
  `$XDG_DATA_HOME/applications/ewe-mobile-app-<package>.desktop` with
  `Name=<App name>`, `GenericName=On <phone name>`, `Comment=Opens on this PC from your phone`,
  `Icon=<C2 icon path>`, `Exec=ewe-mobile --open-app <package>`,
  `StartupWMClass=ewe-mobile-app-<package>`, `Categories=X-EweMobilePhone;`,
  `X-Ewe-Mobile-App=<deviceId>`, `NoDisplay=false` while the setting
  "Show the phone's apps in the launcher" is on (default on), true otherwise
  (the entry stays for the icon in the dock/Overview). Plus one
  `ewe-mobile-screen.desktop` (`NoDisplay=true`, the phone glyph). Entries of
  packages that left the list are deleted; all of them go when the phone is
  unpaired. eep's card rules decide whether these get cards — check
  `eep/docs/CARD.md`; they must **not** become `~/Applications` cards
  (mark them so eep skips them, or ask the orchestrator).
- The shell (Hyprland rules in `ewe/dotfiles/hypr/hyprland.lua`):
  `ewe-mobile-screen` floats, centred, sized to the phone's aspect at ~80 %
  of the screen height; `ewe-mobile-app-*` tile like any app.

### C5 — phone notifications in the shell

`kdeconnectd` 26.08.3 posts each phone notification through
`org.freedesktop.Notifications` (KNotification, event `notification`,
component `kdeconnect`). Read from its source
(`plugins/notifications/notification.cpp`):

- always: hints `x-kde-origin-name` = the phone's name and
  `x-kdeconnect-source-device` = the phone's name; the pixmap (`image-data`)
  is whatever icon the phone sent (C1: the app's launcher icon from now on).
- **only if the server advertises `x-kde-display-appname`** in its
  capabilities: summary = the notification's own title (the sender), body =
  its text, hint `x-kde-display-appname` = the phone app's name. Otherwise
  summary = the app's name and body = "title: text".
- a reply is an **inline reply** if the server supports it, else a regular
  "Reply" action that opens KDE Connect's own Qt dialog — the KDE window to
  get rid of. Quickshell: `NotificationServer.inlineReplySupported`,
  `Notification.hasInlineReply / inlineReplyPlaceholder / sendInlineReply()`,
  `extraHints` for the capability.
- other kdeconnectd notifications (pairing request, incoming call, file or
  text received, low battery) come with `appName` "KDE Connect" and desktop
  entry `org.kde.kdeconnect.daemon`; show them as phone cards named
  **ewe Mobile**, never "KDE Connect".

The card (owner's pick): header = the phone app's icon (rounded tile) + its
name + time + a **phone chip** (phone glyph + phone name); title = sender;
body = text; an inline reply field when offered; **outline in the accent
colour** (`Theme.accent`, `borderWidth1`, or 2 if 1 reads too faint) instead
of `borderSubtle`; no picture on the right (the image is the app icon). Groups
key on phone + app, not "KDE Connect". A click opens
`ewe-mobile --page notifications` when ewe Mobile is installed. The same
marking in Quick settings' notification list.

### C6 — no KDE Connect window anywhere

- PC: the ewe.phone plugin hides `org.kde.kdeconnect.app`, `.sms`,
  `.nonplasma` (indicator), `.handler` and the settings module from the
  launcher (the house's way — eep's hide if it applies to these, else a
  `NoDisplay` copy in `~/.local/share/applications` marked
  `X-Ewe-Phone-Hides=…`), reversibly, behind a plugin setting
  `hide_kdeconnect` (default true). Nothing that would open a KDE Connect
  window is offered by any ewe surface (reply = inline, settings = ewe Mobile).
- Phone: the stock app is uninstalled by the orchestrator once ewe's build
  works (owner's decision); ewe Mobile for Android hides every upstream screen
  behind its own.

## The work

| # | Workstream | Repo · branch | Owner |
|---|---|---|---|
| A1 | **The ewe face** (B2): Compose theme generated from `ewe/design/system/tokens.json`, Geist + Lucide, components, navigation; Home (the computer at a glance + actions), Devices (pair, add by IP, rename), Setup (every permission, restricted settings, local network, accessibility, battery), Settings (per-device features and their options, trusted networks, about); our launcher activity replaces upstream's | `ewe-connector-android` · `b2-ewe-face` | Fable |
| A3 | **The ewe face, feature screens**: touchpad + keyboard, media (the computer's players), commands, send (files, clipboard, text, link), find/ring, presenter — on A1's kit | same branch, after A1's kit lands | Fable/Sonnet |
| A2 | **Apps from the phone** (B3): EweAppsPlugin replacing the spike (C1), launch from the background, package-change resends; notifications carry the app icon | `ewe-connector-android` · `b3-apps` (worktree `~/Projects/ewe/.wt/android-b3-apps`) | Sonnet |
| D1 | **ewe Mobile on the PC**: the adb link (find, pair with the 6-digit code, connect, status), Screen page (mirror in scrcpy's window, options), Apps page (C1/C2: search, grid with icons, open here / on the phone), C3 verbs, C4 windows and launcher entries; polish across the window | `ewe-mobile` · `feat/ecosystem` | Fable |
| S1 | **Phone notifications in the shell** (C5) + the scrcpy window rules (C4) | `ewe` · `feat/phone-notifications` | Sonnet |
| S2 | **The phone in the dock** (ewe.phone): a `dock-item` + popup — phone header (name, battery, in reach), Apps (search, grid from C2, open here / on phone through C3), Files (mount, quick folders into ewe Files: Camera, Screenshots, Downloads, Documents, Music, Internal storage), Mirror screen, Open ewe Mobile; the bridge reads C1 and writes C2; C6 on the PC | `ewe-plugin-phone` · `feat/dock-apps` | Sonnet |
| O | Orchestration: contracts, reviews, merging A2 → A1, the phone end to end, uninstalling the stock app, vault + RFC status | all | this session |

### Order

1. **Thursday night:** A1, A2, D1, S1, S2 in parallel against these contracts
   (fakes for whatever the others have not delivered yet).
2. **Friday:** A2 merged into A1's branch; A3 on A1's kit; end-to-end on the
   phone — apps list → cache → dock, launcher, Apps page → open here / on the
   phone; notifications from the real phone; screen. Then the stock app goes.
3. **Saturday:** polish pass on every surface with screenshots (phone and
   nested shell), the vault, version bumps, the owner's review. Release
   (push, tags, APK) only when the owner says.

### Status (2026-11-26, 19:00)

| # | State | Where |
|---|---|---|
| A1 + A3 + A2 | **merged** on `b2-ewe-face` (874cdb14): ewe flavor, generated tokens, kit, Home/Devices/Setup/Settings, Remote/Media/Commands/Send/Ring/Presenter, EweAppsPlugin, app icon in notifications. 148 unit tests, lint, token checks green; emulator `ember36` only — **never on the phone yet** | `ewe-connector-android` |
| A4 | polish pass running (Home duplicate name, Setup density, Devices unpair, Geist Mono, rounded icon mask, one demo mode) | same branch |
| D1 | done, 3 commits: adb link, Screen + Apps pages, C3 verbs, C4 entries (`X-Eep-Skip=true`), icons on Notifications. Tests green against fakes | `ewe-mobile` · `feat/ecosystem` |
| S1 | done, 2 commits: phone cards, inline reply, ewe Mobile icon fallback, click → `ewe-mobile --page notifications`, `ewe-mobile-screen` window rule | `ewe` · `feat/phone-notifications` |
| S2 | done, 4 commits, 1.2.0: dock item + popup (Apps, Files), bridge C1→C2, C6 hide (`hide_kdeconnect`), wording. `test.sh` 28/28 | `ewe-plugin-phone` · `feat/dock-apps` |
| eep | `X-Eep-Skip=true` → no card (66f6f72) | `eep` · `feat/skip-mark` (worktree `.wt/eep-skip`; eep's main checkout has someone else's uncommitted work on `feat/add-appimage-meta` — leave it) |

**On the phone (2026-11-26, 19:10–19:20, a Pixel on Android 17), all passed:** the merged build
(b2-ewe-face 0c0999a9) installed over the old one, pairing and the accessibility service kept; the C1 list
reached kdeconnectd — 82 launchable apps (31 system), 604 KB, icons 96 px, rounded (`scripts/apps-dump.py`);
`triggerCommand(com.google.android.calculator)` opened Calculator on the phone (background start through the
accessibility service, unlocked phone); `ewe-mobile --adb-status` picked the phone over the emulator;
`--apps` wrote C2 (82 icons); `--open-app … --here` gave a resizable window, class
`ewe-mobile-app-com.google.android.calculator`, title "Calculator"; `--screen` → class `ewe-mobile-screen`,
title = the phone's name (tiled on the live session — the float rule is on the shell branch); phone
notifications now carry the app's launcher icon (Messenger, Messages, Wallet, Instagram, Weather, Calendar —
no initials). The stock app is **uninstalled** from the phone; the PC lists one paired phone (`ab38d75f…`).

**Deploy order (learned the hard way):** eep with `X-Eep-Skip` (`feat/skip-mark`) **before** the new
ewe-mobile. Any ewe-mobile run (even `--apps`) writes the 82 launcher entries; the installed eep 0.4.0 made
82 `~/Applications` cards from them within a minute. Cleaned up the same evening (entries removed → eep
retired the cards itself; 71 cards, no hide copies). ewe-mobile's PKGBUILD should require the eep that has
the rule.

**Not yet done:** deploying to the live desktop (plugin 1.2.0, shell branch, ewe-mobile build), the vault (decisions, Roadmap, Version Ledger, Troubleshooting), version bumps, the release — all with the owner.

### Rules every agent follows

- Exclusive files: only your repo/branch above. Shared docs (this file,
  RFC-012, the vault) are the orchestrator's.
- Commit on your branch, never push, never touch `main`/`b1-fork`.
- The phone is shared: before each test, reinstall **your** APK
  (`adb install -r`); another agent may have installed theirs. Never
  `am force-stop` either app (Android switches the accessibility service
  off), never `cat` the app's `shared_prefs` (they hold the private key),
  never uninstall anything from the phone.
- The shell is tested only in the nested harness (`ewe/.claude/skills/run-ewe/driver.sh`,
  `HS_PRIVATE_BUS=1` for notifications); never `colorscheme.sh`, never the
  live `~/.config`, never `ewe-plugin` verbs without `--no-restart` and the
  Hyprland/Wayland variables unset.
- Design: the ewe design system (`ewe/design/system/`, read
  `guidelines/40-implementation.md`); tokens only, no raw colours or sizes.
