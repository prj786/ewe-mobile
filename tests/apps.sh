#!/usr/bin/env bash
# The phone's apps, end to end on a private bus: a stand-in kdeconnectd with
# one phone whose `remotecommands` serves the C1 fixture, a fake adb and
# scrcpy on PATH, a throwaway HOME for the cache (C2) and the launcher
# entries (C4). Two halves:
#
#   1. cargo test private_bus_apps — the Rust side reads the list over the
#      bus, writes the cache and the entries, triggers a command, and sees
#      `commandsChanged` as the `apps` event.
#   2. the built binary's C3 verbs (--apps, --open-app, --screen,
#      --adb-status): one JSON line each, exit 0, scrcpy started with the
#      class and arguments of C4, nothing started on the real bus.
#
#   tests/apps.sh [path-to-binary]      default: src-tauri/target/release/ewe-mobile
#
# Needs dbus-run-session and python's dbus + gi; without them it says so and
# exits 0. The binary half is skipped when there is no binary.
set -euo pipefail
cd "$(dirname "$0")/.."
command -v dbus-run-session >/dev/null || { echo "skip  no dbus-run-session"; exit 0; }
python3 -c 'import dbus, gi' 2>/dev/null || { echo "skip  python dbus/gi missing"; exit 0; }
bin="${1:-src-tauri/target/release/ewe-mobile}"
repo="$PWD"

work="$(mktemp -d -t ewe-mobile-apps.XXXXXX)"
export work bin repo
trap 'rm -rf "$work"' EXIT
# cargo and rustup live under the real home; the test's HOME is a throwaway
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export HOME="$work/home" XDG_CACHE_HOME="$work/home/.cache" XDG_DATA_HOME="$work/home/.local/share" XDG_RUNTIME_DIR="$work/run"
mkdir -p "$HOME/.android" "$XDG_RUNTIME_DIR"
printf '\n\x13adb-FAKE0001-abcdef' > "$HOME/.android/adb_known_hosts.pb"
export PATH="$repo/tests/fakebin:$PATH"
export FAKE_ADB_LOG="$work/adb.log" FAKE_SCRCPY_LOG="$work/scrcpy.log" FAKE_SCRCPY_SLEEP=2
export EWE_MOBILE_TEST_BUS=private EWE_MOBILE_FIXTURE="$repo/tests/fixtures/c1-apps.json" EWE_MOBILE_TRIGGER_LOG="$work/trigger.log"
export EWE_MOBILE_NO_DAEMON=1

fail=0
ok()  { echo "ok    $1"; }
bad() { echo "FAIL  $1"; fail=1; }
json() { python3 -I -c 'import json,sys; v=json.loads(sys.stdin.read()); print(eval(sys.argv[1]))' "$1"; }

# ── 1. the Rust side ────────────────────────────────────────────────────────
if dbus-run-session -- cargo test --manifest-path src-tauri/Cargo.toml private_bus_apps -- --ignored --nocapture 2>&1 | tail -20 | grep -q "test result: ok"; then
  ok "cargo test private_bus_apps"
else
  bad "cargo test private_bus_apps"
fi

# ── 2. the binary's verbs ───────────────────────────────────────────────────
if [ ! -x "$bin" ]; then
  echo "skip  the C3 verbs (no binary at $bin — build it with: npm run tauri build -- --no-bundle)"
  [ "$fail" = 0 ] && echo "all good"
  exit $fail
fi

verbs() {
  # inside dbus-run-session: the stand-in, then each verb
  python3 "$repo/tests/standin-daemon.py" "$EWE_MOBILE_FIXTURE" "$EWE_MOBILE_TRIGGER_LOG" > "$work/standin.log" 2>&1 &
  local pid=$!
  for _ in $(seq 1 50); do grep -q "stand-in up" "$work/standin.log" 2>/dev/null && break; sleep 0.1; done
  "$bin" --apps                              > "$work/apps.json"
  "$bin" --adb-status                        > "$work/adb-status.json"
  "$bin" --open-app com.whatsapp --phone     > "$work/open-phone.json"
  "$bin" --open-app com.whatsapp             > "$work/open-auto.json"
  "$bin" --open-app nope.not.there --phone   > "$work/open-unknown.json"
  "$bin" --open-app 'bad;name' --here        > "$work/open-bad.json"
  FAKE_ADB_ASLEEP=1 FAKE_ADB_LOG="$work/adb-asleep.log" FAKE_SCRCPY_LOG="$work/scrcpy-asleep.log" \
    "$bin" --open-app org.signal --here      > "$work/open-asleep.json"
  "$bin" --screen                            > "$work/screen.json"
  "$bin" --apps --device nobody              > "$work/apps-nobody.json"
  busctl --user list --acquired 2>/dev/null | grep -c "^org.kde.kdeconnect " > "$work/owners" || true
  sleep 2.5 # the fake scrcpy windows end
  kill $pid 2>/dev/null; wait $pid 2>/dev/null || true
}
dbus-run-session -- bash -c "$(declare -f verbs); verbs"

for f in apps adb-status open-phone open-auto open-unknown open-bad open-asleep screen apps-nobody; do
  [ "$(wc -l < "$work/$f.json")" -le 1 ] && ok "--${f%%-*} is one line" || bad "--$f is one line: $(cat "$work/$f.json")"
done
[ "$(json 'v["ok"] and v["device"]=="abc123" and v["deviceName"]=="Pixel 9" and v["reachable"] and v["adb"] and v["source"]=="phone"' < "$work/apps.json")" = True ] \
  && ok "--apps: the phone, in reach, adb connected, list from the phone" || bad "--apps: $(cat "$work/apps.json")"
[ "$(json '[a["package"] for a in v["apps"]]' < "$work/apps.json")" = "['ge.bog.mobilebank', 'com.android.settings', 'org.signal', 'com.webp', 'com.whatsapp']" ] \
  && ok "--apps: five apps, sorted by name, the hostile key dropped (a bad icon costs only the icon)" || bad "--apps packages: $(json '[a["package"] for a in v["apps"]]' < "$work/apps.json")"
icon="$(json '[a["icon"] for a in v["apps"] if a["package"]=="com.whatsapp"][0]' < "$work/apps.json")"
[ "$icon" = "$XDG_CACHE_HOME/ewe-mobile/abc123/icons/com.whatsapp.png" ] && [ -s "$icon" ] && ok "--apps: C2 icon path, file written" || bad "--apps icon: $icon"
[ -f "$XDG_CACHE_HOME/ewe-mobile/abc123/apps.json" ] && ok "C2 apps.json written" || bad "C2 apps.json missing"
[ ! -e "$XDG_CACHE_HOME/ewe-mobile/abc123/icons/com.webp.png" ] && ok "C2: no icon file for a non-PNG icon" || bad "C2 wrote a webp"
e="$XDG_DATA_HOME/applications/ewe-mobile-app-com.whatsapp.desktop"
[ -f "$e" ] && grep -q "^Exec=ewe-mobile --open-app com.whatsapp$" "$e" && grep -q "^StartupWMClass=ewe-mobile-app-com.whatsapp$" "$e" \
  && grep -q "^X-Ewe-Mobile-App=abc123$" "$e" && grep -q "^GenericName=On Pixel 9$" "$e" && grep -q "^Icon=$icon$" "$e" \
  && ok "C4: the launcher entry says what C4 says" || bad "C4 entry: $(cat "$e" 2>&1)"
[ -f "$XDG_DATA_HOME/applications/ewe-mobile-screen.desktop" ] && grep -q "^NoDisplay=true$" "$XDG_DATA_HOME/applications/ewe-mobile-screen.desktop" \
  && ok "C4: ewe-mobile-screen.desktop, hidden" || bad "C4: no screen entry"
[ "$(json 'v["ok"] and v["tools"]["adb"] and v["tools"]["scrcpy"] and v["paired"] and v["connected"] and "FAKE0001" in v["serial"]' < "$work/adb-status.json")" = True ] \
  && ok "--adb-status: tools, paired, connected, the serial" || bad "--adb-status: $(cat "$work/adb-status.json")"
[ "$(json 'v["ok"] and v["where"]=="phone"' < "$work/open-phone.json")" = True ] && ok "--open-app --phone: went to the phone" || bad "--open-app --phone: $(cat "$work/open-phone.json")"
grep -qx "com.whatsapp" "$work/trigger.log" && ok "… and the daemon got triggerCommand(com.whatsapp)" || bad "no triggerCommand: $(cat "$work/trigger.log" 2>&1)"
[ "$(json 'v["ok"] and v["where"]=="here" and v["class"]=="ewe-mobile-app-com.whatsapp"' < "$work/open-auto.json")" = True ] \
  && ok "--open-app (auto, adb connected): its own window here" || bad "--open-app auto: $(cat "$work/open-auto.json")"
[ "$(json 'v["ok"] is False and v["error"]=="unknown-app"' < "$work/open-unknown.json")" = True ] && ok "--open-app of an app the phone lacks: unknown-app" || bad "$(cat "$work/open-unknown.json")"
[ "$(json 'v["ok"] is False and v["error"]=="unknown-app"' < "$work/open-bad.json")" = True ] && ok "--open-app of a non-package: unknown-app" || bad "$(cat "$work/open-bad.json")"
[ "$(json 'v["ok"] and v["class"]=="ewe-mobile-screen" and v["title"]=="Pixel 9"' < "$work/screen.json")" = True ] && ok "--screen: class ewe-mobile-screen, the phone's name" || bad "--screen: $(cat "$work/screen.json")"
[ "$(json 'v["ok"] is False and v["error"]=="no-device"' < "$work/apps-nobody.json")" = True ] && ok "--apps --device nobody: no-device" || bad "$(cat "$work/apps-nobody.json")"
# what scrcpy was really started with
grep -q "^SDL_APP_ID=ewe-mobile-screen$" "$work/scrcpy.log" && grep -q "^ARG --window-title=Pixel 9$" "$work/scrcpy.log" \
  && ok "scrcpy (screen): SDL_APP_ID=ewe-mobile-screen, title = the phone's name" || bad "scrcpy screen args: $(cat "$work/scrcpy.log")"
grep -q "^SDL_APP_ID=ewe-mobile-app-com.whatsapp$" "$work/scrcpy.log" && grep -q "^ARG --start-app=+com.whatsapp$" "$work/scrcpy.log" \
  && grep -q "^ARG --new-display=1280x800/160$" "$work/scrcpy.log" && grep -q "^ARG --flex-display$" "$work/scrcpy.log" && grep -q "^ARG --no-vd-system-decorations$" "$work/scrcpy.log" \
  && grep -q "^ARG --window-title=WhatsApp$" "$work/scrcpy.log" && grep -q "^ARG --serial=adb-FAKE0001-abcdef._adb-tls-connect._tcp$" "$work/scrcpy.log" \
  && ok "scrcpy (app): class ewe-mobile-app-com.whatsapp, --start-app=+pkg, its own display, title = app name" || bad "scrcpy app args: $(cat "$work/scrcpy.log")"
grep -q "^ARG --keep-active$" "$work/scrcpy.log" && ok "scrcpy (app): --keep-active (the phone stays awake, the window never goes black)" || bad "no --keep-active: $(cat "$work/scrcpy.log")"
grep -q "^adb -s adb-FAKE0001-abcdef._adb-tls-connect._tcp shell dumpsys power" "$work/adb.log" && ok "the phone's screen state asked before the app opens" || bad "no screen-state question: $(cat "$work/adb.log")"
! grep -q "KEYCODE_WAKEUP" "$work/adb.log" && ! grep -q "^ARG --turn-screen-off$" "$work/scrcpy.log" \
  && ok "a phone in use is left alone (no wake key, its screen stays on)" || bad "woke or darkened a phone in use: $(grep -h "WAKEUP\|turn-screen-off" "$work/adb.log" "$work/scrcpy.log")"
[ "$(json 'v["ok"] and v["where"]=="here"' < "$work/open-asleep.json")" = True ] \
  && grep -q "^adb -s adb-FAKE0001-abcdef._adb-tls-connect._tcp shell input keyevent KEYCODE_WAKEUP$" "$work/adb-asleep.log" \
  && grep -q "^ARG --keep-active$" "$work/scrcpy-asleep.log" && grep -q "^ARG --turn-screen-off$" "$work/scrcpy-asleep.log" \
  && ok "the phone asleep: woken first, kept awake, its own screen left dark" || bad "asleep: $(cat "$work/open-asleep.json" "$work/adb-asleep.log" "$work/scrcpy-asleep.log" 2>&1)"
! grep -q "kill-server" "$work/adb.log" && ok "a headless verb never kills the adb server" || bad "kill-server from a verb"
[ "$(cat "$work/owners")" = 1 ] && ok "the stand-in was the only kdeconnect on the private bus" || bad "owners: $(cat "$work/owners")"

[ "$fail" = 0 ] && echo "all good"
exit $fail
