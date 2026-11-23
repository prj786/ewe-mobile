#!/usr/bin/env bash
# Whose word does the window take? Only the daemon's.
#
# On a private session bus (dbus-run-session — never the real one) a stand-in
# owns org.kde.kdeconnect and reports a battery level, while an impostor with
# no name sends the same signal with another level, to everyone and straight
# at every client. The app's signal pump must pass the first and drop the
# second (src-tauri/src/events.rs, `pump`).
#
#   tests/origin.sh
#
# Needs dbus-run-session and python's dbus + gi (both on every ewe machine);
# without them it says so and exits 0.
set -euo pipefail
cd "$(dirname "$0")/.."
command -v dbus-run-session >/dev/null || { echo "skip  no dbus-run-session"; exit 0; }
python3 -c 'import dbus, gi' 2>/dev/null || { echo "skip  python dbus/gi missing"; exit 0; }
exec dbus-run-session -- env EWE_MOBILE_TEST_BUS=private \
  cargo test --manifest-path src-tauri/Cargo.toml private_bus_origin -- --ignored --nocapture
