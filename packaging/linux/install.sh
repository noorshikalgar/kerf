#!/usr/bin/env sh
# Installs Kerf for the current user: ~/.local/bin/kerf, desktop entry and icons.
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
prefix="${PREFIX:-$HOME/.local}"
install -Dm755 "$here/kerf" "$prefix/bin/kerf"
install -Dm644 "$here/kerf.desktop" "$prefix/share/applications/kerf.desktop"
for size in 16 32 64 128 256 512; do
  install -Dm644 "$here/icons/kerf-$size.png" "$prefix/share/icons/hicolor/${size}x${size}/apps/kerf.png"
done
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$prefix/share/applications" || true
echo "Installed kerf to $prefix/bin (make sure it is on your PATH)."
