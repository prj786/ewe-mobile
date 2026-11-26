#!/usr/bin/env bash
# The command-line contract: `--version`, `--status` and the C3 verbs
# (`--apps`, `--open-app`, `--screen`, `--adb-status`) print one JSON line
# and exit 0 (rule 5), and none of them starts kdeconnectd. tests/apps.sh
# runs the verbs against a stand-in daemon; here the bus is empty.
#
#   tests/smoke.sh [path-to-binary]      default: src-tauri/target/release/ewe-mobile
#
# `--status` runs on a private session bus (dbus-run-session), so this never
# talks to — or starts — the daemon of the session that runs the test.
set -euo pipefail
cd "$(dirname "$0")/.."
bin="${1:-src-tauri/target/release/ewe-mobile}"
[ -x "$bin" ] || { echo "no binary at $bin — build it with: npm run tauri build -- --no-bundle" >&2; exit 2; }
fail=0
ok()   { echo "ok    $1"; }
bad()  { echo "FAIL  $1"; fail=1; }
json() { python3 -I -c 'import json,sys; v=json.loads(sys.stdin.read()); print(eval(sys.argv[1]))' "$1"; }

out="$("$bin" --version)" && ok "--version exits 0" || bad "--version exits 0"
[ "$(printf '%s' "$out" | wc -l)" -eq 0 ] && ok "--version is one line" || bad "--version is one line"
[ "$(printf '%s' "$out" | json 'v["ok"] and v["name"] == "ewe-mobile" and v["product"] == "ewe Mobile" and bool(v["version"])')" = "True" ] \
  && ok "--version names the app" || bad "--version names the app: $out"

if command -v dbus-run-session >/dev/null; then
  out="$(dbus-run-session -- "$bin" --status)" && ok "--status exits 0" || bad "--status exits 0"
  [ "$(printf '%s' "$out" | json 'v["ok"] is True and v["running"] is False and v["devices"] == []')" = "True" ] \
    && ok "--status on an empty bus: not running, no devices" || bad "--status on an empty bus: $out"
  # the private bus must still have no kdeconnectd after being asked
  # (--acquired: a plain list also names every service that COULD be started)
  out="$(dbus-run-session -- sh -c "\"$bin\" --status >/dev/null; busctl --user list --acquired 2>/dev/null | grep -c org.kde.kdeconnect || true")"
  [ "${out:-0}" = "0" ] && ok "--status did not start the daemon" || bad "--status started the daemon"
  # the C3 verbs on an empty bus: no phone, said in JSON, exit 0, no daemon
  for verb in "--apps" "--open-app com.whatsapp" "--screen"; do
    out="$(dbus-run-session -- "$bin" $verb)" && ok "$verb exits 0" || bad "$verb exits 0"
    [ "$(printf '%s' "$out" | json 'v["ok"] is False and v["error"] == "no-device" and bool(v["message"])')" = "True" ] \
      && ok "$verb on an empty bus: no-device, in words" || bad "$verb on an empty bus: $out"
  done
  out="$(dbus-run-session -- "$bin" --adb-status)" && ok "--adb-status exits 0" || bad "--adb-status exits 0"
  [ "$(printf '%s' "$out" | json 'v["ok"] is True and "adb" in v["tools"] and "scrcpy" in v["tools"] and "paired" in v and "connected" in v')" = "True" ] \
    && ok "--adb-status names the tools and the link" || bad "--adb-status: $out"
  out="$(dbus-run-session -- sh -c "\"$bin\" --apps >/dev/null; \"$bin\" --adb-status >/dev/null; busctl --user list --acquired 2>/dev/null | grep -c org.kde.kdeconnect || true")"
  [ "${out:-0}" = "0" ] && ok "the verbs did not start the daemon" || bad "a verb started the daemon"
else
  echo "skip  --status (no dbus-run-session)"
fi

[ "$fail" = 0 ] && echo "all good"
exit $fail
