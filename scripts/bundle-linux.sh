#!/usr/bin/env bash
# Builds dist/Kerf-<version>-linux-<arch>.tar.gz: binary, desktop entry, icons, installer.
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
ARCH="$(uname -m)"
NAME="Kerf-${VERSION}-linux-${ARCH}"
cargo build --release
rm -rf "dist/$NAME" && mkdir -p "dist/$NAME/icons"
cp target/release/kerf "dist/$NAME/"
cp packaging/linux/kerf.desktop packaging/linux/install.sh README.md "dist/$NAME/"
for size in 16 32 64 128 256 512; do cp "assets/icon/kerf-$size.png" "dist/$NAME/icons/"; done
tar -C dist -czf "dist/$NAME.tar.gz" "$NAME"
echo "built dist/$NAME.tar.gz"
