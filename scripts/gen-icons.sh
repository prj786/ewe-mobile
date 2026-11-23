#!/usr/bin/env bash
# Rasterise the SVG art into the PNGs Tauri and the package need.
#   packaging/ewe-mobile.svg → src-tauri/icons/{32,64,128,256}x*.png, icon.png,
#                              128x128@2x.png (Tauri's expected names)
# Needs rsvg-convert (librsvg). Run after editing the SVG; the PNGs are
# committed so a checkout builds without librsvg.
set -euo pipefail
cd "$(dirname "$0")/.."
command -v rsvg-convert >/dev/null || { echo "rsvg-convert (librsvg) is required" >&2; exit 1; }
out=src-tauri/icons
mkdir -p "$out"
for s in 32 64 128 256; do
    rsvg-convert -w "$s" -h "$s" packaging/ewe-mobile.svg -o "$out/${s}x${s}.png"
done
cp "$out/256x256.png" "$out/128x128@2x.png"
rsvg-convert -w 512 -h 512 packaging/ewe-mobile.svg -o "$out/icon.png"
ls -1 "$out"
