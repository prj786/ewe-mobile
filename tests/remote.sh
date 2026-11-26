#!/usr/bin/env bash
# Does the daemon understand what the Remote page sends?
#
# On a private session bus (dbus-run-session — never the real one) a stand-in
# owns org.kde.kdeconnect with one phone's pointer and keyboard on it, and
# prints every call with the signature it arrived with. The app's own
# functions (src-tauri/src/kdc.rs: point, command, press) call it; the test
# holds each call against what kdeconnect 26.08 declares — moveCursor (ii),
# sendCommand a{sv}, sendKeyPress sibbbbb — and waits for the phone's answer
# to a key to come back as a `keyEcho` event.
#
#   tests/remote.sh
#
# Needs dbus-run-session and python's dbus + gi (both on every ewe machine);
# without them it says so and exits 0.
set -euo pipefail
cd "$(dirname "$0")/.."
command -v dbus-run-session >/dev/null || { echo "skip  no dbus-run-session"; exit 0; }
python3 -c 'import dbus, gi' 2>/dev/null || { echo "skip  python dbus/gi missing"; exit 0; }
exec dbus-run-session -- env EWE_MOBILE_TEST_BUS=private \
  cargo test --manifest-path src-tauri/Cargo.toml private_bus_remote -- --ignored --nocapture
