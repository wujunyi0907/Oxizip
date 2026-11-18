#!/usr/bin/env bash
# Build release binaries and produce .deb / .AppImage (and optional .rpm).
# Prerequisites:
#   - Rust toolchain, g++, pkg-config, X11/Wayland GUI deps
#   - cargo install cargo-packager --locked
# Optional RPM: apt/yum install rpm / rpmbuild, or use fpm (see below)

set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release -p oxizip-cli -p oxizip-gui

cargo packager --release --formats deb,appimage --binaries-dir target/release

echo "Packages under: dist/"
ls -la dist/*.{deb,AppImage} 2>/dev/null || ls -la dist/

# Optional: convert deb → rpm when alien or fpm is available.
if command -v alien >/dev/null 2>&1; then
  shopt -s nullglob
  for deb in dist/*.deb; do
    (cd dist && sudo alien -r --scripts "$(basename "$deb")") || true
  done
elif command -v fpm >/dev/null 2>&1; then
  # Extract deb payload into a staging tree and wrap as rpm.
  work=$(mktemp -d)
  trap 'rm -rf "$work"' EXIT
  deb=$(ls dist/*.deb | head -n1)
  dpkg-deb -x "$deb" "$work/root"
  dpkg-deb -e "$deb" "$work/control"
  ver=$(grep -E '^Version:' "$work/control/control" | awk '{print $2}' | cut -d- -f1)
  fpm -s dir -t rpm -n oxizip -v "${ver:-0.1.0}" -C "$work/root" \
    --description "Oxizip archive tool" \
    --license "AGPL-3.0-only" \
    -p dist/
fi

ls -la dist/*.rpm 2>/dev/null || true
