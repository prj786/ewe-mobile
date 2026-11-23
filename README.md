# ewe Mobile

Your phone on your desktop: messages, contacts, notifications, what is
playing, files and a remote — one ewe window instead of KDE Connect's five.

ewe Mobile is a **window over KDE Connect's daemon**. `kdeconnectd` keeps the
link to the phone, the pairing keys and the caches; ewe Mobile is everything
you see of it, drawn with the ewe design system. The phone runs the stock
KDE Connect app. (An ewe app for Android is planned beside it; until then
nothing on the phone changes.)

| Page | What it does |
| --- | --- |
| **Phone** | The device at a glance: battery, mobile signal, ring it, send the clipboard or files, open its storage, the newest messages and notifications |
| **Messages** | SMS and MMS as conversations: names and photos from the phone's contacts, picture attachments, links, search, new message to a contact or a number. A sent message shows as "Sending…" until the phone confirms it |
| **Contacts** | The phone's address book: search, message, have the phone dial, copy a number. Readable while the phone is away |
| **Notifications** | What the phone is showing, by app: dismiss (on the phone too), reply to the ones that take a reply |
| **Media** | The phone's player: play, pause, skip, seek, volume, which player |
| **Files** | Send files (drop them anywhere on the window), text and links; browse the phone's storage in ewe Files; open what the phone sent |
| **Remote** | This computer's mouse and keyboard on the phone |
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
- `src/lib/state.svelte.js` — the window's reading of the daemon, and its
  actions. Screens are in `src/lib/screens/`.

ewe Mobile keeps nothing: no database, no copy of your messages. Close the
window and the process ends. The one thing remembered is which phone was in
use (the webview's local storage).

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
  not a page that waits for good.

Two facts about KDE Connect's D-Bus interface that cost time to find
(verified on kdeconnect 26.08):

- SMS is on the **device** object (`…/devices/<id>`, interface
  `org.kde.kdeconnect.device.conversations`) — there is no `/conversations`
  child. A message is the **struct** `(i s a(s) x i i x i x a(xsss))` in a
  variant: event, body, addresses, date (ms), type (1 received, 2 sent),
  read, thread id, uid, SIM id, attachments.
- `replyToConversation` does nothing, silently, for a thread the daemon has
  not cached. `send_message` checks and falls back to the addresses.

## Command line

```
ewe-mobile                 open the window (a second launch focuses it)
ewe-mobile --page <id>     … on a page: phone messages contacts notifications media files remote devices
ewe-mobile --status        {"ok":true,"installed":…,"running":…,"devices":[…]}   (never starts the daemon)
ewe-mobile --version       {"ok":true,"name":"ewe-mobile","product":"ewe Mobile","version":"…"}
```

`--status` and `--version` print one JSON line and exit 0, like every ewe
tool. The page ids are public: they never change with the labels.

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
tests/smoke.sh                    # the built binary's --version / --status contract
cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

Build through the Tauri CLI for anything you run (`npm run tauri build --
--no-bundle` → `src-tauri/target/release/ewe-mobile`): a bare `cargo build`
bakes the Vite dev URL into the binary.

The mock (`src/lib/devmock.js`) takes parameters: `scene=welcome|pairing|
asks|away|empty|nodaemon|two`, `scheme=ewe-dark|ewe-light|a11y`,
`thread=<1…6>`, `call=1`, `keyboard=1`, `nosshfs=1`, `still`. Everyone in it
is made up.

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

## Licence

MIT. Geist is OFL, Lucide is ISC (licences in `src/assets/`).
