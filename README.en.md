# Oxizip

[简体中文](README.md)

A **clean-room** multi-format archive tool in Rust: one shared engine, with both a **GUI** and a **CLI**. Targets **Windows x64** and **Linux x86_64** (**64-bit only**).

| | |
|---|---|
| License | [AGPL-3.0-only](LICENSE) |
| Application ID | `cn.wujunyi.oxizip` |
| Version | See root [`Cargo.toml`](Cargo.toml) / [CHANGELOG](CHANGELOG.md) |
| Releases | [GitHub Releases](https://github.com/wujunyi0907/Oxizip/releases) (tags must be `vX.Y.Z`) |

---

## Features

- **Unified engine**: list, extract, create, test, repair, password recovery, and more — shared by GUI and CLI
- **Desktop (iced)**: rounded borderless windows; entries open with the system default app (**no in-app preview**); dedicated settings window and shell job progress dialogs
- **CLI**: `list` / `extract` / `create` / `test` / `detect` / `add` / `delete` / `rename` / `checksum` / `repair` / `recover-password` / `shell`
- **Path safety**: zip-slip protection on extract
- **Progress & cancel**: long jobs report progress and can be cancelled
- **Shell integration**: `oxizip-cli shell install` — Windows classic flat verbs + Win11 Sparse cascade (`oxizip-shellext`); Linux `.desktop` / Nautilus / KDE
- **Settings & associations**: file associations, context-menu toggles, extract/create prefs; menu filter written to `shell_menu.json`
- **Editable language packs**: `locales/*.toml` next to the install or under the user config dir; restart after edits

### Format support

| | Create (compress) | Extract / list |
|---|---|---|
| In-tree / compliant crates | `zip` · `7z` · `zipx` · `exe` (SFX) · `tar` · `tgz` · `lzh` · `iso` · `gz` · `xz` · `zpaq` | ZIP / ZIPX / EXE(SFX) / TAR family / GZ·XZ·ZST·BZ2 / 7Z / LZH / ISO / ZPAQ / CAB / ALZ / EGG |
| External CLI | — | **RAR** (official UnRAR, extract only) · **WIM** (wimlib-imagex, extract only) |

Also: best-effort **repair** for ZIP / 7z; **password recovery** for ZIP / 7z (dictionary + optional short brute-force); checksums CRC32 / SHA-256 (Windows can write Zone.Identifier).

---

## Install

Prefer downloads from [Releases](https://github.com/wujunyi0907/Oxizip/releases).

### Windows

Artifact: `Oxizip-Setup-<version>-x64.exe` (custom iced install / uninstall UI).

- Default path: `C:\Program Files\Oxizip` (UAC elevation)
- Ships `oxizip.exe` (GUI), `oxizip-cli.exe` (CLI), format icons, plus `lib/unrar` and `lib/wimlib`
- Post-install association wizard; uninstaller is `Uninstall.exe` in the install dir
- Ship the Setup; do not treat a bare `oxizip.exe` as the installer

Local packaging:

```powershell
.\scripts\package-windows.ps1
```

### Linux

| Artifact | Install / uninstall |
|---|---|
| `Oxizip_<ver>_amd64.deb` | `sudo dpkg -i …` / `sudo apt remove oxizip` |
| `Oxizip-<ver>-1.x86_64.rpm` | `sudo rpm -Uvh …` / `sudo rpm -e oxizip` |
| `Oxizip-<ver>-x86_64.AppImage` | `chmod +x` then run; delete the file to remove |
| `Oxizip-<ver>-linux-x86_64.tar.gz` | unpack the `usr/` tree |

System package name is lowercase `oxizip`; distributed filenames use a capital `Oxizip` prefix. RPM keeps the NEVRA `-1` Release segment. Layout includes `/usr/bin/oxizip`, `oxizip-cli`, `oxizip.desktop`, docs, icons, and `/usr/share/oxizip/locales/`. RAR / WIM need system `unrar` and `wimlib-imagex` (Recommends on deb).

Local packaging (ImageMagick required for AppImage PNG icons):

```bash
chmod +x scripts/package-linux.sh
./scripts/package-linux.sh
```

For offline builds set `OXIZIP_FETCH_APPIMAGE_TOOLS=0` and provide `appimagetool` / `linuxdeploy` yourself.

### CI releases

Push a tag **`vX.Y.Z`** (e.g. `v0.1.8`; `Cargo.toml` keeps `0.1.8` without `v`) or run [Package](.github/workflows/package.yml) manually. A GitHub Release is created **only if** Setup, deb, rpm, AppImage, and tar.gz are **all** present; any missing artifact fails the job and skips the Release. See [AGENTS.md](AGENTS.md).

---

## Language & fonts

- CLI / GUI: **简体中文** and **English** (`--lang zh-cn|en`; switchable in the GUI)
- Locale files: `crates/oxizip-i18n/locales/*.toml`; at runtime also merged from  
  - `locales/` next to the executable  
  - Linux: `/usr/share/oxizip/locales/`  
  - User: `%APPDATA%\Oxizip\locales\` or `~/.config/oxizip/locales/`
- Windows GUI: “Microsoft YaHei UI” by default; Release builds hide the console
- Windows CLI: UTF-8 console code page
- Linux GUI: install CJK fonts (e.g. `fonts-noto-cjk`)

---

## Runtime external tools

Oxizip does **not** modify or link these tools’ sources/libraries; it only spawns them when needed.

| Tool | Role | Windows lookup | Other |
|---|---|---|---|
| **UnRAR** | RAR extract only | `OXIZIP_UNRAR` → next to app → **`lib/unrar/UnRAR.exe`** → `PATH` | `OXIZIP_UNRAR` → system `unrar` |
| **wimlib-imagex** | WIM extract only | `OXIZIP_WIMLIB` → next to app → **`lib/wimlib/`** → `PATH` | `OXIZIP_WIMLIB` → system `wimlib-imagex`, etc. |

The Windows installer ships the official unmodified trees from [`lib/unrar/`](lib/unrar/) and [`lib/wimlib/`](lib/wimlib/) plus their license texts. Missing tools yield localizable errors.

**ZPAQ** does not spawn an external exe: sources under [`lib/zpaq/`](lib/zpaq/) are built by `oxizip-formats` with `-DOXIZIP_EMBEDDED`; only the embed entry macros may change. Keep `lib/zpaq/COPYING` in releases.

---

## Development

Requires a Rust toolchain (`rustc` / `cargo`). Conventions: [AGENTS.md](AGENTS.md).

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p oxizip-cli -- --help    # → oxizip-cli(.exe)
cargo run -p oxizip-gui              # → oxizip(.exe)
```

Binaries: **GUI = `oxizip`**, **CLI = `oxizip-cli`**.

```bash
cargo run -p oxizip-cli -- c out.zip ./some_file
cargo run -p oxizip-cli -- c -f 7z -p secret -m lzma2 out.7z ./some_file
cargo run -p oxizip-cli -- c -f zipx -m zstd --volume-size 1048576 out.zipx ./big_dir
cargo run -p oxizip-cli -- l out.zip --password secret
cargo run -p oxizip-cli -- x out.7z.001 -o ./out_dir --password secret
cargo run -p oxizip-cli -- repair damaged.zip -o repaired.zip
cargo run -p oxizip-cli -- shell install
```

### Repository layout

```
crates/
  oxizip-core/       # progress, cancel, errors, path safety
  oxizip-formats/    # format probing and readers/writers
  oxizip-engine/     # unified facade
  oxizip-cli/        # CLI
  oxizip-gui/        # iced UI
  oxizip-i18n/       # strings (locales/*.toml)
  oxizip-setup/      # Windows install / uninstall UI
  oxizip-sfx-stub/   # SFX stub
  oxizip-shellext/   # Windows Explorer extension (spawns CLI only)
lib/                 # unrar · wimlib (Windows ship) · zpaq (embed)
packaging/           # desktop file, Sparse manifest, deb scripts, …
samples/             # self-made test archives
scripts/             # package-windows.ps1 · package-linux.sh
```

Dependency direction: `cli / gui → engine → formats → core` (no reverse deps). `reference/` is local-only (gitignore) and must not enter the build.

---

## License

Oxizip is released under the **GNU Affero General Public License v3.0 only (AGPL-3.0-only)**; see [LICENSE](LICENSE). The tables below list licenses of major dependencies / external tools and do not change Oxizip’s AGPL obligations. Full transitive deps: `Cargo.lock` and crate metadata; the GUI “Licenses” page also shows a summary.

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

- Local materials under `reference/` (gitignore)
- Temporary binaries / SDK under `third_party/unrar/` and `third_party/wimlib/` (gitignore; DLLs are not linked)
- Canonical tree: [`lib/`](lib/) — `unrar/`, `wimlib/`, `zpaq/`

### Pre-release checklist

1. Ship `LICENSE` (AGPL-3.0); if you offer a modified version as a network service, provide corresponding complete source per AGPL  
2. Keep `lib/zpaq/COPYING`  
3. Keep UnRAR / wimlib license texts under `lib/unrar/` and `lib/wimlib/`; wimlib sources at [wimlib.net](https://wimlib.net/downloads/)  
4. Do not distribute `ark*.dll` or unauthorized proprietary libraries  
