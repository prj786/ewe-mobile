# ewe Mobile

Your phone on your desktop: messages, contacts, notifications, what is
playing, files, its screen, its apps and a remote — one ewe window instead
of KDE Connect's five.

ewe Mobile is a **window over KDE Connect's daemon**. `kdeconnectd` keeps the
link to the phone, the pairing keys and the caches; ewe Mobile is everything
you see of it, drawn with the ewe design system. On the phone, **ewe Mobile for
Android** (a fork of KDE Connect's app with ewe's face,
`prj786/ewe-connector-android`) sends its apps and their icons as well; the
stock KDE Connect app still works for everything but the Apps page. The
plan and the contracts between the pieces: `docs/ECOSYSTEM.md`.

| Page | What it does |
| --- | --- |
| **Phone** | The device at a glance: battery, mobile signal, ring it, send the clipboard or files, open its storage, the newest messages and notifications |
| **Messages** | SMS and MMS as conversations: names and photos from the phone's contacts, picture attachments, links, search, new message to a contact or a number. A sent message shows as "Sending…" until the phone confirms it |
| **Contacts** | The phone's address book: search, message, have the phone dial, copy a number. Readable while the phone is away |
| **Notifications** | What the phone is showing, by app: dismiss (on the phone too), reply to the ones that take a reply |
| **Media** | The phone's player: play, pause, skip, seek, volume, which player |
| **Files** | Send files (drop them anywhere on the window), text and links; browse the phone's storage in ewe Files; open what the phone sent |
| **Screen** | The phone's screen in a window on this computer (scrcpy over adb). Says what it costs first, pairs with the six-digit code once, then one button: Mirror — with the phone's screen off, sound here, stay awake and a quality pick |
| **Apps** | The phone's apps with their real icons (from ewe Mobile for Android), kept while the phone is away, search; a click opens an app in its own window here when the debugging link is up, else on the phone; a menu offers both. Each app can also sit in the launcher (Super+D) |
| **Remote** | This computer's mouse and keyboard on the phone: a pad for the pointer (tap, swipe, long-press, turn the page, Back / Home / Recents) and a box that types into the field open on the phone. The phone has to allow it once — see below |
| **Devices** | Pair and unpair, pick the phone in use, name this computer, switch single features on and off |

Not here, on purpose: the bar's phone glyph and the Quick settings page.
Those are layer-shell surfaces, which a Tauri window cannot be — they are the
[`ewe.phone`](https://github.com/prj786/ewe-plugin-phone) plugin, in the shell.
Its page opens this app for everything it has no room for.

## How it is built

```
phone ── KDE Connect protocol ── kdeconnectd ── session D-Bus ── ewe-mobile (Rust) ── invoke / events ── the window (Svelte)
```

- `src-tauri/src/bus.rs` — the session bus (zbus). A reply becomes JSON once;
  nothing models Qt's types.
- `src-tauri/src/kdc.rs` — every command the window can run.
- `src-tauri/src/events.rs` — one match rule for the whole
  `/modules/kdeconnect` tree; every signal becomes one `kdc` event.
- `src-tauri/src/contacts.rs` — the vCards KDE Connect caches in
  `~/.local/share/kpeoplevcard/kdeconnect-<id>/` (vCard 2.1 and 3.0).
- `src-tauri/src/apps.rs` — the phone's apps: the list the phone sends (C1),
  checked key by key; the cache every reader shares (C2); the launcher
  entries (C4). `src-tauri/src/adb.rs` — the debugging link: adb over
  Wi-Fi, scrcpy's windows. `src-tauri/src/screen.rs` — the two pages'
  commands and the headless verbs (C3). The contracts are
  `docs/ECOSYSTEM.md`.
- `src/lib/state.svelte.js` — the window's reading of the daemon, and its
  actions. Screens are in `src/lib/screens/`.
- `src/lib/remote.js` — what a press, a drag, a wheel and a key mean on the
  Remote page, before anything is sent (plain functions; `npm test`).
- `src/lib/apps.js` — the Apps page's search, letter tiles and the package
  behind a notification (plain functions; `npm test`).

ewe Mobile keeps almost nothing: no database, no copy of your messages.
Close the window and the process ends. What stays: which phone was in use
and the mirror's options (the webview's local storage), the apps cache in
`~/.cache/ewe-mobile/<deviceId>/` (C2 — so the dock and the launcher work
while the phone is away), the launcher entries it writes under
`~/.local/share/applications/ewe-mobile-*.desktop`, and one setting,
`mobile.launcher_apps`, in ewe.conf through `ewe-conf`. All of it goes
when the phone is unpaired.

### What it takes on trust, and what it doesn't

The page shows text that comes from a phone — messages, notification text,
contact names — so the Rust side treats every argument as a string someone
else may have written:

- **Only the daemon's signals count.** The bus hands a signal to whoever it
  is addressed to, whatever the sender calls itself; each one is checked
  against the address that owns `org.kde.kdeconnect` at that moment
  (`events.rs`, `pump`; `tests/origin.sh` runs an impostor against it).
- **Ids never leave their path.** Device, notification and feature ids are
  letters, digits and `_` before they reach an object path; a contact id is
  one file name inside the device's own folder.
- **Files open only if the phone sent them:** an attachment the daemon
  fetched, or a received file that was written in the last minutes (the
  daemon also says "received" for a link the phone shared, and a link can
  name any file).
- **Files are sent only if someone chose them:** the picker runs in Rust and
  its choice never passes through the page; a dropped file is one the window
  system reported as dropped.
- **Links are web links** (`http`/`https`), the dialler gets digits, and
  nothing goes through a shell.
- **Every call has a time limit**, so a hung daemon is an error on the page,
  not a page that waits for good. adb and scrcpy get limits too (10 s for
  a question, 25 s for pairing or connecting, 60 s for `--list-apps`).
- **The apps list is a phone's word.** A key is a package only if it is
  `[A-Za-z0-9._]{1,255}` with no leading, trailing or doubled dot; a name
  is one clean line of at most 200 characters; an icon is a square PNG of
  16–512 px under 256 KB or it is no icon. A list without the `"ewe"`
  marker is someone's real command list and is never shown as apps. A
  package also has to be in the list before `--open-app` sends it to the
  phone.
- **adb's key stays adb's** (`~/.android`), the pairing is one `adb pair
  host:port <six digits>` with a host adb will take and nothing else, and
  the only thing read back is whether `adb_known_hosts.pb` names the
  phone's guid. Nothing of it enters ewe.conf.

Two facts about KDE Connect's D-Bus interface that cost time to find
(verified on kdeconnect 26.08):

- SMS is on the **device** object (`…/devices/<id>`, interface
  `org.kde.kdeconnect.device.conversations`) — there is no `/conversations`
  child. A message is the **struct** `(i s a(s) x i i x i x a(xsss))` in a
  variant: event, body, addresses, date (ms), type (1 received, 2 sent),
  read, thread id, uid, SIM id, attachments.
- `replyToConversation` does nothing, silently, for a thread the daemon has
  not cached. `send_message` checks and falls back to the addresses.
- `remotekeyboard.sendKeyPress` takes **seven** arguments since 26.08 —
  `key, specialKey, shift, ctrl, alt, super, sendAck`. With six, the last is
  read as `super`. `tests/remote.sh` holds the call to the signature.

### The Remote page and the phone

The pointer and the keys go out whether or not anyone is listening. A phone
takes them only once **KDE Connect is an accessibility service** there (KDE
Connect → the plugins that "need permissions" → Mouse receiver → Accessibility
settings; a copy from outside the Play Store first needs "Allow restricted
settings" in its App info). Until then it drops everything without a word,
and the page looks dead. Two things do come back, and the page is built on
them (KDE Connect for Android 1.36 or newer):

- `remoteState` is true while the phone takes input — the accessibility
  service is on, or its own keyboard is up. False: the page shows what to
  switch on, on the phone.
- every key the phone really typed is acknowledged (`keyPressReceived`). Keys
  that get no answer went nowhere — no text field is open there — and the
  page says that instead of echoing them as if they had landed.

What the phone makes of the pointer's words is KDE Connect's, not ours:
`singleclick` taps, `singlehold` / `singlerelease` put the finger down and
lift it, `middleclick` is Home, `backclick` Back, `forwardclick` Recents,
`rightclick` the menu key on Android 16+ (Back before it). A `scroll` is not
a distance: the phone adds `dy` up and turns one whole page when the sum
passes 500, forward when it is positive — so the wheel sends whole pages.

### Screen and apps (docs/ECOSYSTEM.md, C1–C4)

The phone's screen and its apps come over a second link beside KDE
Connect's: **adb over Wi-Fi** (Android's Wireless debugging, paired once
with a six-digit code, found by mDNS afterwards) with **scrcpy** drawing
the windows. Both are optional packages (`android-tools`, `scrcpy`); when
they are missing the Screen page hands them to Komble. The Screen page
says what the link costs before it asks for anything: developer options,
a paired computer that can do anything on the phone, and the banking apps
that refuse while developer options is on (TBC and Liberty do; Bank of
Georgia does not).

- **C1**: ewe Mobile for Android announces the launchable apps as its
  `remotecommands` command list — a JSON object with an `"ewe"` marker and
  one entry per package (name, base64 PNG launcher icon, `sys`). The stock
  KDE Connect app sends none: the Apps page says to install ewe Mobile on
  the phone, and with adb connected offers `scrcpy --list-apps` (12.5 s a
  call, so it is cached a day) as a list without icons.
- **C2**: `$XDG_CACHE_HOME/ewe-mobile/<deviceId>/apps.json` and
  `icons/<package>.png`, written atomically whenever the list arrives or
  changes (`commandsChanged`), sorted by name; the dock popup and the
  launcher entries read the same files.
- **C3**: the verbs below. **C4**: scrcpy is started with `SDL_APP_ID`
  (scrcpy 5 is SDL3), so the mirrored screen has the class
  `ewe-mobile-screen` with the phone's name as its title, and one app has
  `ewe-mobile-app-<package>` with the app's name — Hyprland floats the
  first at a phone's shape and tiles the rest. One launcher entry per app
  is written as C4 lists it, hidden (`NoDisplay=true`, kept for the dock's
  icon) while "Show the phone's apps in the launcher" is off.

"Open here" is `scrcpy --new-display=1280x800/160 --flex-display
--no-vd-system-decorations --start-app=+<package>`: a virtual display of
its own that follows the window, and `+` stops the app first so one that is
open on the phone moves here. scrcpy's windows are their own processes and
outlive this app; an adb server this app started is stopped when the
window closes, unless a window of ours still needs it.

**eep and the entries.** eep's card rules (`eep/docs/CARD.md`) give every
launcher app a `~/Applications` card, and have no skip mark yet. The
entries carry `Categories=X-EweMobilePhone;`, `X-Ewe-Mobile-App=<deviceId>`
and `X-Eep-Skip=true` so eep can learn to pass them over in one line;
until it does, a visible phone app also gets a card.

## Command line

```
ewe-mobile                 open the window (a second launch focuses it)
ewe-mobile --page <id>     … on a page: phone messages contacts notifications media files screen apps remote devices
ewe-mobile --status        {"ok":true,"installed":…,"running":…,"devices":[…]}   (never starts the daemon)
ewe-mobile --version       {"ok":true,"name":"ewe-mobile","product":"ewe Mobile","version":"…"}

ewe-mobile --apps [--device <id>]
    {"ok":true,"device":"<id>","deviceName":"Pixel 10 Pro XL","reachable":true,"adb":true,
     "source":"phone|cache|none","updated":1764170000000,
     "apps":[{"package":"com.whatsapp","name":"WhatsApp","sys":false,"icon":"/home/u/.cache/ewe-mobile/<id>/icons/com.whatsapp.png"}]}
ewe-mobile --open-app <package> [--here|--phone] [--device <id>]
    {"ok":true,"package":"com.whatsapp","where":"here","class":"ewe-mobile-app-com.whatsapp","pid":1234}
    {"ok":true,"package":"com.whatsapp","where":"phone","class":null,"pid":null}
    --here: its own window here (needs adb); --phone: on the phone's screen (C1); neither: here when adb is connected, else phone
ewe-mobile --screen [--device <id>]
    {"ok":true,"class":"ewe-mobile-screen","title":"Pixel 10 Pro XL","pid":1234}
ewe-mobile --adb-status [--device <id>]
    {"ok":true,"tools":{"adb":true,"scrcpy":true,"packages":["android-tools","scrcpy"]},"paired":true,"connected":true,
     "serial":"adb-…._adb-tls-connect._tcp","model":"Pixel 10 Pro XL","services":[{"name":"adb-…","kind":"connect","host":"192.168.1.5","port":39855}],"devices":[…]}

a failure: {"ok":false,"error":"no-device|away|no-adb-tools|adb-not-paired|adb-not-connected|unknown-app|scrcpy-failed","message":"<words>"}
```

Every verb prints one JSON line and exits 0, like every ewe tool. The four
of C3 open no window, never start `kdeconnectd` (they talk to the daemon's
own address, which the bus never activates), and spawn scrcpy detached.
`--device` picks a phone by its KDE Connect id; without it the paired
phone in reach is taken, else the paired one. The page ids are public:
they never change with the labels.

`EWE_MOBILE_NO_DAEMON=1` stops the app from starting `kdeconnectd` — the bus
starts it on any method call, and a test run on a private bus would become a
new device on your network.

## Develop

```
npm install
npm run tauri dev                 # the real app against your phone
npm run dev                       # the UI alone: http://localhost:5177/?mock=1#messages
npm test                          # node: numbers, times, SMS parts, links
cd src-tauri && cargo test        # Rust: vCards, the message decoder, events, ids
tests/origin.sh                   # a private bus: only the daemon's signals pass
tests/remote.sh                   # a private bus: the Remote page's calls, as the daemon gets them
tests/smoke.sh                    # the built binary's --version / --status / C3 verbs on an empty bus
tests/apps.sh                     # a private bus: a stand-in daemon serving the C1 fixture, a fake adb and
                                  #   scrcpy on PATH; the Rust side end to end, then the binary's C3 verbs
cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

Build through the Tauri CLI for anything you run (`npm run tauri build --
--no-bundle` → `src-tauri/target/release/ewe-mobile`): a bare `cargo build`
bakes the Vite dev URL into the binary.

The mock (`src/lib/devmock.js`) takes parameters: `scene=welcome|pairing|
asks|away|empty|nodaemon|two`, `scheme=ewe-dark|ewe-light|a11y`,
`thread=<1…6>`, `call=1`, `keyboard=1` (the phone takes the pointer and
keys), `deaf=1` (… but answers no key), `nosshfs=1`,
`adb=none|tools|seen|pairing|paired|connected` (the debugging link's
state; `123456` is the code that pairs), `stock=1` (the stock app: no apps
list), `mirror=1`, `still`, `trace=1` (every command the page sends, on
the console). Everyone in it is made up.

To look at a page the way Tauri draws it: `npm run dev`, a nested
compositor (`ewe/.claude/skills/run-ewe/driver.sh up`), then a WebKitGTK
view of `http://localhost:5177/?mock=1&still&scheme=ewe-light#apps` with
`WAYLAND_DISPLAY` set to the nested socket, saved through
`WebKit2.WebView.get_snapshot` (a ten-line Python script).

Two tests read the **real** daemon and phone, and are off by default. They
ask only for what opening the app asks, send nothing, and print counts:

```
cd src-tauri
cargo test live_reads   -- --ignored --nocapture   # device, notifications, threads, contacts, media
cargo test live_signals -- --ignored --nocapture   # a conversation's messages, as signals
```

`src/tokens.css` and `src/components.css` are copies of `ewe/design/` — never
edit them here.

## Package

`PKGBUILD` builds the Arch package (Tauri has no pacman bundler); the release
workflow attaches it to the GitHub release for the `[ewe]` repository to pick
up. Depends on `kdeconnect` and `sshfs` (KDE Connect mounts the phone's
storage with it; without it the Files page says what is missing).
`android-tools` and `scrcpy` are optional: the Screen and Apps pages work
without them as far as the phone alone allows, and offer them through
Komble.

## Licence

MIT. Geist is OFL, Lucide is ISC (licences in `src/assets/`).
