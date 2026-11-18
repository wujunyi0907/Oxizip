#!/usr/bin/env bash
# Build release binaries and produce system packages for Linux:
#   - dist/Oxizip_<ver>_<debarch>.deb   (dpkg-deb; installs under /usr)
#   - dist/Oxizip-<ver>-1.<rpmarch>.rpm (rpmbuild/fpm; keep RPM NEVRA Release in the filename)
#   - dist/Oxizip-<ver>-<arch>.AppImage (portable; appimagetool / linuxdeploy)
#   - dist/Oxizip-<ver>-linux-<arch>.tar.gz (same FHS tree, portable)
#
# Prerequisites:
#   - Rust toolchain, g++, pkg-config, X11/Wayland GUI deps
#   - dpkg-deb (for .deb)
#   - rpmbuild and/or fpm (for .rpm; optional)
#   - appimagetool and/or linuxdeploy (for AppImage; auto-fetched unless
#     OXIZIP_FETCH_APPIMAGE_TOOLS=0)
#   - ImageMagick `convert`/`magick` optional (app icon PNG; needed for AppImage icon)

set -euo pipefail
cd "$(dirname "$0")/.."

# AppImage tools often need FUSE; extract-and-run works without it.
export APPIMAGE_EXTRACT_AND_RUN="${APPIMAGE_EXTRACT_AND_RUN:-1}"
OXIZIP_FETCH_APPIMAGE_TOOLS="${OXIZIP_FETCH_APPIMAGE_TOOLS:-1}"

VER=$(grep -E '^version\s*=' Cargo.toml | head -n1 | sed -E 's/.*"([^"]+)".*/\1/')
HOST_ARCH=$(uname -m)
case "$HOST_ARCH" in
  x86_64|amd64)
    DEB_ARCH=amd64
    RPM_ARCH=x86_64
    TAR_ARCH=x86_64
    APPIMAGE_ARCH=x86_64
    ;;
  aarch64|arm64)
    DEB_ARCH=arm64
    RPM_ARCH=aarch64
    TAR_ARCH=aarch64
    APPIMAGE_ARCH=aarch64
    ;;
  *)
    echo "Unsupported architecture: $HOST_ARCH (64-bit only)" >&2
    exit 1
    ;;
esac

PKG_ROOT="target/package/linux-root"
DEB_ROOT="target/package/deb"
RPM_BUILD="target/package/rpm"
APPDIR="target/package/AppDir"
TOOLS_DIR="target/package/tools"
DIST="dist"
ICO="crates/oxizip-gui/assets/icons/oxizip.ico"

echo "==> Building release binaries..."
cargo build --release -p oxizip-cli -p oxizip-gui

echo "==> Staging FHS tree under $PKG_ROOT ..."
rm -rf "$PKG_ROOT" "$DEB_ROOT" "$RPM_BUILD" "$APPDIR"
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
  # Dump ICO frames; pick the largest PNG as the source.
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

# --- tar.gz (portable FHS tree) ---
TAR_NAME="Oxizip-${VER}-linux-${TAR_ARCH}"
TAR_OUT="$DIST/${TAR_NAME}.tar.gz"
echo "==> Writing $TAR_OUT"
tar -C "$PKG_ROOT" -czf "$TAR_OUT" usr

# --- .deb ---
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

  # Installed-Size in KiB
  local size_kb
  size_kb=$(du -sk "$root" | awk '{print $1}')
  printf 'Installed-Size: %s\n' "$size_kb" >>"$root/DEBIAN/control"

  local deb_out="$DIST/Oxizip_${VER}_${DEB_ARCH}.deb"
  echo "==> Building $deb_out"
  dpkg-deb --root-owner-group --build "$root" "$deb_out"
}
build_deb

# --- .rpm ---
build_rpm() {
  local rpm_out=""
  if command -v rpmbuild >/dev/null 2>&1; then
    echo "==> Building .rpm via rpmbuild"
    mkdir -p "$RPM_BUILD"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
    local spec="$RPM_BUILD/SPECS/oxizip.spec"
    local date_str
    date_str=$(date '+%a %b %d %Y')

    # Dynamic %files from staged tree (skip empty icon dirs).
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
      # Keep RPM NEVRA: Name-Version-Release.Arch.rpm (Release is required).
      # Only capitalize the display name prefix for Release assets.
      rpm_out="$DIST/Oxizip-${VER}-1.${RPM_ARCH}.rpm"
      cp -f "$built" "$rpm_out"
    fi
  elif command -v fpm >/dev/null 2>&1; then
    echo "==> Building .rpm via fpm"
    rpm_out="$DIST/Oxizip-${VER}-1.${RPM_ARCH}.rpm"
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

# --- AppImage ---
download_file() {
  local url=$1
  local dest=$2
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL -o "$dest" "$url"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$dest" "$url"
  else
    echo "Need curl or wget to download $url" >&2
    return 1
  fi
}

ensure_tool() {
  # ensure_tool <name> <url> → prints path to executable
  local name=$1
  local url=$2
  local path=""
  if command -v "$name" >/dev/null 2>&1; then
    command -v "$name"
    return 0
  fi
  mkdir -p "$TOOLS_DIR"
  path="$TOOLS_DIR/$name"
  # Drop broken symlinks left by older script versions.
  if [[ -L "$path" && ! -e "$path" ]]; then
    rm -f "$path"
  fi
  if [[ -f "$path" && -x "$path" ]]; then
    echo "$path"
    return 0
  fi
  if [[ "$OXIZIP_FETCH_APPIMAGE_TOOLS" != "1" ]]; then
    return 1
  fi
  echo "    fetching $name ..." >&2
  download_file "$url" "$path" || return 1
  chmod +x "$path"
  echo "$path"
}

find_app_icon() {
  local p
  for p in \
    "$PKG_ROOT/usr/share/pixmaps/oxizip.png" \
    "$PKG_ROOT/usr/share/icons/hicolor/256x256/apps/oxizip.png" \
    "$PKG_ROOT/usr/share/icons/hicolor/128x128/apps/oxizip.png" \
    "$PKG_ROOT/usr/share/icons/hicolor/64x64/apps/oxizip.png"
  do
    if [[ -f "$p" ]]; then
      echo "$p"
      return 0
    fi
  done
  return 1
}

build_appimage() {
  echo "==> Building AppImage"
  local icon
  if ! icon=$(find_app_icon); then
    echo "    Skip AppImage (need PNG icon; install ImageMagick and re-run)"
    return 0
  fi

  local appimagetool="" linuxdeploy="" plugin=""
  local at_url ld_url plug_url
  at_url="https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-${APPIMAGE_ARCH}.AppImage"
  ld_url="https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-${APPIMAGE_ARCH}.AppImage"
  plug_url="https://github.com/linuxdeploy/linuxdeploy-plugin-appimage/releases/download/continuous/linuxdeploy-plugin-appimage-${APPIMAGE_ARCH}.AppImage"

  appimagetool=$(ensure_tool appimagetool "$at_url" || true)
  # Prefer bundling shared libs when linuxdeploy is available / fetchable.
  linuxdeploy=$(ensure_tool linuxdeploy "$ld_url" || true)
  if [[ -n "$linuxdeploy" ]]; then
    plugin=$(ensure_tool linuxdeploy-plugin-appimage "$plug_url" || true)
    if [[ -n "$plugin" ]]; then
      # Discover by PATH name (do NOT symlink the file onto itself).
      export PATH="${TOOLS_DIR}:${PATH:-}"
      # Also place a copy next to linuxdeploy when it lives elsewhere.
      local ld_dir
      ld_dir=$(dirname "$linuxdeploy")
      if [[ "$ld_dir" != "$TOOLS_DIR" ]]; then
        cp -f "$plugin" "$ld_dir/linuxdeploy-plugin-appimage"
        chmod +x "$ld_dir/linuxdeploy-plugin-appimage"
      fi
    fi
  fi

  if [[ -z "$appimagetool" && ( -z "$linuxdeploy" || -z "$plugin" ) ]]; then
    echo "    Skip AppImage (need appimagetool, or linuxdeploy + plugin)"
    echo "    Set OXIZIP_FETCH_APPIMAGE_TOOLS=1 and ensure curl/wget, or install tools on PATH"
    return 0
  fi

  rm -rf "$APPDIR"
  mkdir -p "$APPDIR"
  cp -a "$PKG_ROOT/." "$APPDIR/"

  # AppImage root metadata
  install -m 644 packaging/oxizip.desktop "$APPDIR/oxizip.desktop"
  # Exec must use a bare binary name (no absolute path) for AppImage / linuxdeploy.
  sed -i 's|^Exec=.*|Exec=oxizip %F|' "$APPDIR/oxizip.desktop"
  install -m 644 "$icon" "$APPDIR/oxizip.png"
  ln -sfn oxizip.png "$APPDIR/.DirIcon"

  cat >"$APPDIR/AppRun" <<'EOF'
#!/bin/sh
set -e
SELF=$(readlink -f "$0")
HERE=${SELF%/*}
export PATH="${HERE}/usr/bin:${PATH:-}"
export XDG_DATA_DIRS="${HERE}/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
# Prefer bundled libs when linuxdeploy populated usr/lib.
if [ -d "${HERE}/usr/lib" ]; then
  export LD_LIBRARY_PATH="${HERE}/usr/lib:${LD_LIBRARY_PATH:-}"
fi
if [ -d "${HERE}/usr/lib64" ]; then
  export LD_LIBRARY_PATH="${HERE}/usr/lib64:${LD_LIBRARY_PATH:-}"
fi
exec "${HERE}/usr/bin/oxizip" "$@"
EOF
  chmod +x "$APPDIR/AppRun"

  local out="$DIST/Oxizip-${VER}-${APPIMAGE_ARCH}.AppImage"
  rm -f "$out"

  pack_with_appimagetool() {
    ARCH="$APPIMAGE_ARCH" "$appimagetool" "$APPDIR" "$out"
  }

  local built=""
  if [[ -n "$linuxdeploy" && -n "$plugin" ]]; then
    echo "    bundling with linuxdeploy..."
    set +e
    "$linuxdeploy" \
      --appdir "$APPDIR" \
      --executable "$APPDIR/usr/bin/oxizip" \
      --executable "$APPDIR/usr/bin/oxizip-cli" \
      --desktop-file "$APPDIR/oxizip.desktop" \
      --icon-file "$APPDIR/oxizip.png" \
      --output appimage
    local ld_rc=$?
    set -e
    if [[ $ld_rc -eq 0 ]]; then
      built=$(ls -1t Oxizip-*.AppImage oxizip-*.AppImage 2>/dev/null | head -n1 || true)
      if [[ -z "$built" ]]; then
        built=$(ls -1t ./*.AppImage 2>/dev/null | head -n1 || true)
      fi
    else
      echo "    linuxdeploy failed (exit $ld_rc)"
    fi
    if [[ -n "$built" && -f "$built" ]]; then
      mv -f "$built" "$out"
    elif [[ -n "$appimagetool" ]]; then
      echo "    falling back to appimagetool"
      pack_with_appimagetool
    else
      echo "    AppImage build failed" >&2
      return 1
    fi
  else
    pack_with_appimagetool
  fi

  chmod +x "$out"
  echo "    appimage: $out"
}
build_appimage

echo ""
echo "Done. Artifacts:"
ls -la "$DIST"/Oxizip* 2>/dev/null || ls -la "$DIST" || true
echo ""
echo "Install / uninstall examples:"
echo "  sudo dpkg -i dist/Oxizip_${VER}_${DEB_ARCH}.deb && sudo apt remove oxizip"
echo "  sudo rpm -Uvh dist/Oxizip-*.rpm && sudo rpm -e oxizip"
echo "  chmod +x dist/Oxizip-${VER}-${APPIMAGE_ARCH}.AppImage && ./dist/Oxizip-${VER}-${APPIMAGE_ARCH}.AppImage"
