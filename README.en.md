# Oxizip

[简体中文](README.md)

A **clean-room** multi-format archive tool written in Rust: one shared engine, plus a CLI and a desktop UI.

## Features

- **Unified engine**: list, extract, create, test, repair, and password recovery share one API used by both CLI and GUI
- **CLI**: `clap` subcommands (`list` / `extract` / `create` / `test` / `detect` / `add` / `delete` / `rename` / `checksum` / `repair` / `recover-password` / `shell`)
- **Desktop**: `iced`; entries open with the system default app (no in-app preview); icons live under `crates/oxizip-gui/assets/icons`
- **Path safety**: zip-slip protection on extract
- **Progress & cancel**: long jobs report progress and can be cancelled
- **Shell integration**: `oxizip-cli shell install` (Windows context menus / Linux desktop actions)
- **Architecture**: **64-bit only**

### Create (compress)

`zip` · `7z` · `zipx` · `exe` (SFX) · `tar` · `tgz` · `lzh` · `iso` · `gz` · `xz` · `zpaq` (official sources embedded in-process)

### Extract / list

ZIP / ZIPX / EXE(SFX) / TAR family / GZ·XZ·ZST·BZ2 / 7Z / LZH / ISO / ZPAQ / CAB / ALZ / EGG / WIM (`wimlib` CLI) / RAR (official UnRAR CLI, extract only)

### Other capabilities

- **Repair** (best-effort): damaged ZIP / 7z
- **Password recovery** (ZIP / 7z): dictionary + optional short brute-force
- **Checksums**: CRC32 / SHA-256; on Windows can write Zone.Identifier

## Repository layout

```
crates/
  oxizip-core/       # progress, cancel, errors, path safety
  oxizip-formats/    # format probing and readers/writers
  oxizip-engine/     # unified facade
  oxizip-cli/        # command line
  oxizip-gui/        # iced UI
  oxizip-i18n/       # strings
  oxizip-sfx-stub/   # SFX stub
third_party/         # embedded sources (e.g. ZPAQ) and optional local CLI binaries (gitignore)
samples/             # self-made test archives
```

Dependency direction: `cli/gui → engine → formats → core` (no reverse deps).

## Development

Requires a Rust toolchain (`rustc` / `cargo`).

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p oxizip-cli -- --help          # → oxizip-cli(.exe)
cargo run -p oxizip-gui                    # → oxizip(.exe)
```

Binary names: **GUI = `oxizip`**, **CLI = `oxizip-cli`**.

CLI examples:

```bash
cargo run -p oxizip-cli -- c out.zip ./some_file
cargo run -p oxizip-cli -- c -f 7z -p secret -m lzma2 out.7z ./some_file
cargo run -p oxizip-cli -- c -f zipx -m zstd --volume-size 1048576 out.zipx ./big_dir
cargo run -p oxizip-cli -- l out.zip --password secret
cargo run -p oxizip-cli -- x out.7z.001 -o ./out_dir --password secret
cargo run -p oxizip-cli -- repair damaged.zip -o repaired.zip
cargo run -p oxizip-cli -- shell install
```

Collaboration and coding conventions: [AGENTS.md](AGENTS.md). Release notes: [CHANGELOG.md](CHANGELOG.md).

## UI language & fonts

- CLI / GUI support **简体中文** and **English** (`--lang zh-cn|en`; switchable in the GUI).
- Windows GUI: defaults to “Microsoft YaHei UI”; **Release** builds hide the console window.
- Windows CLI: sets the console code page to UTF-8 to avoid mojibake.
- Linux GUI: needs system CJK fonts (e.g. `fonts-noto-cjk`).

## Packaging

GitHub Actions: push a tag in the form **`vX.Y.Z`** (e.g. `v1.0.1` — the leading `v` is required; `Cargo.toml` keeps `1.0.1` without `v`) or run [Package](.github/workflows/package.yml) manually to build Windows Setup and Linux deb/rpm/AppImage. On such a tag, a GitHub Release is created **only if Setup, deb, rpm, AppImage, and tar.gz are all present**; any missing artifact fails the job and skips the Release (packaging is not run on every `main` push). See [AGENTS.md](AGENTS.md) and [CHANGELOG.md](CHANGELOG.md).

### Windows (custom installer `Oxizip-Setup-<version>-x64.exe`)

Clean-room iced install / uninstall UI (destination folder, license confirm, optional associations):

```powershell
.\scripts\package-windows.ps1
```

Output: `dist\Oxizip-Setup-<version>-x64.exe` (version from root `Cargo.toml` `workspace.package.version`).

- Default install path: `C:\Program Files\Oxizip` (UAC elevation)
- Embeds `oxizip.exe` (GUI), `oxizip-cli.exe` (CLI), and format icons
- After install can launch the app and guide associations; uninstaller is `Uninstall.exe` in the install dir (same UI stack)
- Ship the Setup; do not treat a bare `oxizip.exe` as the installer

### Linux (`.deb` / `.rpm` / AppImage + tar.gz)

On a 64-bit Linux host or container:

```bash
chmod +x scripts/package-linux.sh
./scripts/package-linux.sh
```

| Artifact | Tooling | Install / uninstall |
|---|---|---|
| `dist/Oxizip_<ver>_<arch>.deb` | `dpkg-deb` | `sudo dpkg -i …` / `sudo apt remove oxizip` |
| `dist/Oxizip-<ver>-1.<arch>.rpm` | `rpmbuild` or `fpm` | `sudo rpm -Uvh …` / `sudo rpm -e oxizip` |
| `dist/Oxizip-<ver>-<arch>.AppImage` | `linuxdeploy` + `appimagetool` (auto-fetched by default) | `chmod +x` then run; delete the file to remove |
| `dist/Oxizip-<ver>-linux-<arch>.tar.gz` | tar | manually unpack the `usr/` tree |

Distributed filenames use a capital `Oxizip` prefix. deb/AppImage/tar/exe have no Release segment; **RPM keeps NEVRA** as `Oxizip-<ver>-1.<arch>.rpm` (`-1` is the Release build iteration). The system package name remains lowercase `oxizip` for `apt`/`rpm`.

Layout: `/usr/bin/oxizip`, `/usr/bin/oxizip-cli`, `/usr/share/applications/oxizip.desktop`, docs and icons (PNG from ICO when ImageMagick is available; **AppImage needs a PNG icon**). Missing tools skip that format. For offline builds set `OXIZIP_FETCH_APPIMAGE_TOOLS=0` and put `appimagetool` / `linuxdeploy` on `PATH`.

## Runtime external tools

Oxizip does **not** modify or link these tools’ sources/libraries; it only spawns them via `Command` when needed.

### UnRAR (RAR extract only)

| Platform | Lookup order |
|---|---|
| Windows | `OXIZIP_UNRAR` → next to the app → **`lib/unrar/UnRAR.exe`** → `PATH` |
| Other | `OXIZIP_UNRAR` → system `unrar` |

Missing tool → localizable error. The Windows installer ships the official unmodified 64-bit CLI and its license from repo [`lib/unrar/`](lib/unrar/).

### wimlib (WIM extract only)

| Platform | Lookup order |
|---|---|
| Windows | `OXIZIP_WIMLIB` → next to the app → **`lib/wimlib/wimlib-imagex.exe`** → `PATH` |
| Other | `OXIZIP_WIMLIB` → system `wimlib-imagex` / `wimdir`+`wimextract` |

The Windows installer also ships [`lib/wimlib/`](lib/wimlib/) (`wimlib-imagex.exe` + `libwim-15.dll`; spawn only, no link).

### ZPAQ (embedded in-process)

No external `zpaq.exe`. Sources under `lib/zpaq/` are built by `oxizip-formats` `build.rs` with `-DOXIZIP_EMBEDDED`; only the embed entry macros may be changed. Keep `lib/zpaq/COPYING` in releases.

## License

Oxizip is released under the **GNU Affero General Public License v3.0 only (AGPL-3.0-only)**; see [LICENSE](LICENSE). The table below lists licenses of major dependencies / external tools and does not change Oxizip’s AGPL obligations. Full transitive deps: `Cargo.lock` and crate metadata; the GUI “Licenses” page also shows a summary.

### Embedded / linked

| Component | Role | License |
|---|---|---|
| ZPAQ (`lib/zpaq`) | `.zpaq` read/write | Unlicense; embedded libdivsufsort-lite is MIT |
| cab / lzxd | Cabinet | MIT / MIT OR Apache-2.0 |
| unalz / unegg / libazo | ALZ / EGG | MIT |
| sevenz-turbo | 7z | Apache-2.0 |
| zip / tar / flate2 / bzip2 / lzma-rs | archives & codecs | MIT or MIT OR Apache-2.0 |
| zstd | Zstandard | BSD-3-Clause |
| iced / rfd / open / clap / thiserror, etc. | GUI / CLI | MIT or MIT OR Apache-2.0 |

### External black-box CLIs

| Tool | Role | License notes |
|---|---|---|
| UnRAR (Alexander Roshal) | RAR extract | Freeware; free redistribution; must not be used to reverse-engineer proprietary RAR **compression**; AS IS |
| wimlib / wimlib-imagex | WIM extract | GPLv3+ (libwim may be LGPLv2.1+ when not conflicting); spawn only, no link; bundling requires complying with its terms |

### Not part of the build

- Local materials under `reference/` (gitignore, not built)
- Local temporary `*.exe` / SDK under `third_party/unrar/` and `third_party/wimlib/` (gitignore; DLLs are not linked)
- Canonical tree: [`lib/`](lib/) — `unrar/`, `wimlib/` (installer → `<install>\lib\…`), `zpaq/` (build embed only)

### Pre-release checklist

1. Ship `LICENSE` (AGPL-3.0); if you offer a modified version as a network service, provide corresponding complete source per AGPL
2. Keep `lib/zpaq/COPYING`
3. Keep UnRAR / wimlib license texts under `lib/unrar/` and `lib/wimlib/`; wimlib is GPLv3+, sources at [wimlib.net](https://wimlib.net/downloads/)
4. Do not distribute `ark*.dll` or unauthorized proprietary libraries
