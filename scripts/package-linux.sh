#!/usr/bin/env bash
# Build release binaries and produce system packages for Linux:
#   - dist/oxizip_<ver>_<debarch>.deb   (dpkg-deb; installs under /usr)
#   - dist/oxizip-<ver>-1.<rpmarch>.rpm (rpmbuild or fpm)
#   - dist/oxizip-<ver>-linux-<arch>.tar.gz (same FHS tree, portable)
#
# Prerequisites:
#   - Rust toolchain, g++, pkg-config, X11/Wayland GUI deps
#   - dpkg-deb (for .deb)
#   - rpmbuild and/or fpm (for .rpm; optional)
#   - ImageMagick `convert`/`magick` optional (app icon PNG)

set -euo pipefail
cd "$(dirname "$0")/.."

VER=$(grep -E '^version\s*=' Cargo.toml | head -n1 | sed -E 's/.*"([^"]+)".*/\1/')
HOST_ARCH=$(uname -m)
case "$HOST_ARCH" in
  x86_64|amd64)
    DEB_ARCH=amd64
    RPM_ARCH=x86_64
    TAR_ARCH=x86_64
    ;;
  aarch64|arm64)
    DEB_ARCH=arm64
    RPM_ARCH=aarch64
    TAR_ARCH=aarch64
    ;;
  *)
    echo "Unsupported architecture: $HOST_ARCH (64-bit only)" >&2
    exit 1
    ;;
esac

PKG_ROOT="target/package/linux-root"
DEB_ROOT="target/package/deb"
RPM_BUILD="target/package/rpm"
DIST="dist"
ICO="crates/oxizip-gui/assets/icons/oxizip.ico"

echo "==> Building release binaries..."
cargo build --release -p oxizip-cli -p oxizip-gui

echo "==> Staging FHS tree under $PKG_ROOT ..."
rm -rf "$PKG_ROOT" "$DEB_ROOT" "$RPM_BUILD"
mkdir -p \
  "$PKG_ROOT/usr/bin" \
  "$PKG_ROOT/usr/share/applications" \
  "$PKG_ROOT/usr/share/doc/oxizip" \
  "$PKG_ROOT/usr/share/pixmaps" \
  "$PKG_ROOT/usr/share/icons/hicolor"

install -m 755 target/release/oxizip "$PKG_ROOT/usr/bin/oxizip"
install -m 755 target/release/oxizip-cli "$PKG_ROOT/usr/bin/oxizip-cli"
install -m 644 packaging/oxizip.desktop "$PKG_ROOT/usr/share/applications/oxizip.desktop"
install -m 644 LICENSE "$PKG_ROOT/usr/share/doc/oxizip/copyright"
install -m 644 README.md "$PKG_ROOT/usr/share/doc/oxizip/README.md"

# Optional: rasterize ICO → hicolor / pixmaps (best-effort).
install_icons() {
  local tool=""
  if command -v magick >/dev/null 2>&1; then
    tool=magick
  elif command -v convert >/dev/null 2>&1; then
    tool=convert
  else
    echo "    (skip icons: ImageMagick not found)"
    return 0
  fi
  local tmp
  tmp=$(mktemp -d)
  if [[ "$tool" == magick ]]; then
    magick "$ICO" "$tmp/frame.png" 2>/dev/null || true
  else
    convert "$ICO" "$tmp/frame.png" 2>/dev/null || true
  fi
  local src
  src=$(ls -1S "$tmp"/frame*.png 2>/dev/null | head -n1 || true)
  if [[ -z "$src" ]]; then
    echo "    (skip icons: failed to convert $ICO)"
    rm -rf "$tmp"
    return 0
  fi
  for sz in 16 24 32 48 64 128 256; do
    local dir="$PKG_ROOT/usr/share/icons/hicolor/${sz}x${sz}/apps"
    mkdir -p "$dir"
    if [[ "$tool" == magick ]]; then
      magick "$src" -resize "${sz}x${sz}" "$dir/oxizip.png"
    else
      convert "$src" -resize "${sz}x${sz}" "$dir/oxizip.png"
    fi
  done
  cp -f "$PKG_ROOT/usr/share/icons/hicolor/256x256/apps/oxizip.png" \
    "$PKG_ROOT/usr/share/pixmaps/oxizip.png"
  rm -rf "$tmp"
  echo "    icons installed under /usr/share/icons/hicolor"
}
install_icons

mkdir -p "$DIST"

TAR_NAME="oxizip-${VER}-linux-${TAR_ARCH}"
TAR_OUT="$DIST/${TAR_NAME}.tar.gz"
echo "==> Writing $TAR_OUT"
tar -C "$PKG_ROOT" -czf "$TAR_OUT" usr

build_deb() {
  if ! command -v dpkg-deb >/dev/null 2>&1; then
    echo "==> Skip .deb (dpkg-deb not found)"
    return 0
  fi
  local root="$DEB_ROOT/oxizip_${VER}_${DEB_ARCH}"
  mkdir -p "$root/DEBIAN"
  cp -a "$PKG_ROOT/." "$root/"

  cat >"$root/DEBIAN/control" <<EOF
Package: oxizip
Version: ${VER}
Section: utils
Priority: optional
Architecture: ${DEB_ARCH}
Maintainer: Oxizip Maintainers <oxizip@wujunyi.cn>
Homepage: https://wujunyi.cn
Depends: libc6, libx11-6, libxcb1, libxkbcommon0, libwayland-client0
Recommends: fonts-noto-cjk, unrar, wimtools
Description: Clean-room archive tool (GUI + CLI)
 Oxizip is a clean-room archive utility with an iced GUI and CLI.
 Application ID: cn.wujunyi.oxizip. RAR/WIM extraction uses system
 UnRAR / wimlib-imagex when available.
EOF

  install -m 755 packaging/linux/postinst "$root/DEBIAN/postinst"
  install -m 755 packaging/linux/postrm "$root/DEBIAN/postrm"

  local size_kb
  size_kb=$(du -sk "$root" | awk '{print $1}')
  printf 'Installed-Size: %s\n' "$size_kb" >>"$root/DEBIAN/control"

  local deb_out="$DIST/oxizip_${VER}_${DEB_ARCH}.deb"
  echo "==> Building $deb_out"
  dpkg-deb --root-owner-group --build "$root" "$deb_out"
}
build_deb

build_rpm() {
  local rpm_out=""
  if command -v rpmbuild >/dev/null 2>&1; then
    echo "==> Building .rpm via rpmbuild"
    mkdir -p "$RPM_BUILD"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
    local spec="$RPM_BUILD/SPECS/oxizip.spec"
    local date_str
    date_str=$(date '+%a %b %d %Y')
    local files_list
    files_list=$(cd "$PKG_ROOT" && find usr -type f | sort | sed 's|^|/|')

    cat >"$spec" <<EOF
Name:           oxizip
Version:        ${VER}
Release:        1%{?dist}
Summary:        Clean-room archive tool (GUI + CLI)
License:        AGPL-3.0-only
URL:            https://wujunyi.cn
BuildArch:      ${RPM_ARCH}
AutoReqProv:    no

Requires:       glibc
Requires:       libX11
Requires:       libxcb
Requires:       libxkbcommon
Requires:       libwayland-client

%description
Oxizip is a clean-room archive utility with an iced GUI and a CLI.
Application ID: cn.wujunyi.oxizip

%install
mkdir -p %{buildroot}
cp -a $(pwd)/${PKG_ROOT}/. %{buildroot}/

%files
${files_list}

%post
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q /usr/share/applications || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q /usr/share/icons/hicolor || true
fi

%postun
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q /usr/share/applications || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q /usr/share/icons/hicolor || true
fi

%changelog
* ${date_str} Oxizip Maintainers <oxizip@wujunyi.cn> - ${VER}-1
- Package Oxizip ${VER}
EOF

    rpmbuild \
      --define "_topdir $(pwd)/${RPM_BUILD}" \
      --define "_build_id_links none" \
      -bb "$spec"
    local built
    built=$(find "$RPM_BUILD/RPMS" -name 'oxizip-*.rpm' | head -n1)
    if [[ -n "$built" ]]; then
      rpm_out="$DIST/$(basename "$built")"
      cp -f "$built" "$rpm_out"
    fi
  elif command -v fpm >/dev/null 2>&1; then
    echo "==> Building .rpm via fpm"
    rpm_out="$DIST/oxizip-${VER}-1.${RPM_ARCH}.rpm"
    fpm -s dir -t rpm -n oxizip -v "$VER" \
      --iteration 1 \
      --architecture "$RPM_ARCH" \
      --license "AGPL-3.0-only" \
      --description "Clean-room archive tool (GUI + CLI)" \
      --url "https://wujunyi.cn" \
      --maintainer "Oxizip Maintainers <oxizip@wujunyi.cn>" \
      --depends glibc --depends libX11 --depends libxcb \
      --depends libxkbcommon --depends libwayland-client \
      -C "$PKG_ROOT" \
      -p "$rpm_out" \
      usr
  else
    echo "==> Skip .rpm (need rpmbuild or fpm)"
    return 0
  fi

  if [[ -n "${rpm_out}" && -f "$rpm_out" ]]; then
    echo "    rpm: $rpm_out"
  fi
}
build_rpm

echo ""
echo "Done. Artifacts:"
ls -la "$DIST"/oxizip* 2>/dev/null || true
echo ""
echo "Install / uninstall examples:"
echo "  sudo dpkg -i dist/oxizip_${VER}_${DEB_ARCH}.deb && sudo apt remove oxizip"
echo "  sudo rpm -Uvh dist/oxizip-*.rpm && sudo rpm -e oxizip"
