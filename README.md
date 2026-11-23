# Oxizip

[English](README.en.md)

用 Rust **干净室**实现的跨格式压缩 / 解压工具：一套引擎，同时提供 **GUI** 与 **CLI**。目标平台为 **Windows x64** 与 **Linux x86_64**（仅 64 位）。

| | |
|---|---|
| 许可 | [AGPL-3.0-only](LICENSE) |
| 应用 ID | `cn.wujunyi.oxizip` |
| 当前版本 | 见根目录 [`Cargo.toml`](Cargo.toml) / [CHANGELOG](CHANGELOG.md) |
| 发行包 | [GitHub Releases](https://github.com/wujunyi0907/Oxizip/releases)（标签须为 `vX.Y.Z`） |

---

## 功能概览

- **统一引擎**：列表、解压、创建、校验、修复、密码恢复等 API；GUI 与 CLI 共用
- **桌面端（iced）**：圆角无边框窗口；条目用系统默认程序打开（**不做应用内预览**）；独立设置窗与壳层作业进度窗
- **命令行**：`list` / `extract` / `create` / `test` / `detect` / `add` / `delete` / `rename` / `checksum` / `repair` / `recover-password` / `shell`
- **路径安全**：解压防目录穿越（zip-slip）
- **进度与取消**：长时间任务可报告进度并支持取消
- **壳集成**：`oxizip-cli shell install` — Windows 经典扁平右键 + Win11 Sparse 级联（`oxizip-shellext`）；Linux `.desktop` / Nautilus / KDE
- **设置与关联**：文件关联、右键菜单开关、解压 / 压缩偏好；菜单过滤写入 `shell_menu.json`
- **可编辑语言包**：安装目录或用户配置下的 `locales/*.toml`，增改后重启生效

### 格式支持

| | 创建（压缩） | 解压 / 列表 |
|---|---|---|
| 自研 / 合规库 | `zip` · `7z` · `zipx` · `exe`(SFX) · `tar` · `tgz` · `lzh` · `iso` · `gz` · `xz` · `zpaq` | ZIP / ZIPX / EXE(SFX) / TAR 族 / GZ·XZ·ZST·BZ2 / 7Z / LZH / ISO / ZPAQ / CAB / ALZ / EGG |
| 外部 CLI | — | **RAR**（官方 UnRAR，仅解压）· **WIM**（wimlib-imagex，仅解压） |

其他：ZIP / 7z **修复**（尽力而为）；ZIP / 7z **密码恢复**（词典 + 可选短爆破）；校验和 CRC32 / SHA-256（Windows 可写 Zone.Identifier）。

---

## 安装

优先从 [Releases](https://github.com/wujunyi0907/Oxizip/releases) 下载对应平台安装包。

### Windows

产物：`Oxizip-Setup-<version>-x64.exe`（自研 iced 安装 / 卸载界面）。

- 默认安装到 `C:\Program Files\Oxizip`（需管理员权限）
- 内含 `oxizip.exe`（GUI）、`oxizip-cli.exe`（CLI）、格式图标，以及 `lib/unrar`、`lib/wimlib`
- 安装后可引导文件关联；卸载程序为安装目录下的 `Uninstall.exe`
- 请分发 Setup，不要把单独的 `oxizip.exe` 当作安装包

本地打包：

```powershell
.\scripts\package-windows.ps1
```

### Linux

| 产物 | 安装 / 卸载 |
|---|---|
| `Oxizip_<ver>_amd64.deb` | `sudo dpkg -i …` / `sudo apt remove oxizip` |
| `Oxizip-<ver>-1.x86_64.rpm` | `sudo rpm -Uvh …` / `sudo rpm -e oxizip` |
| `Oxizip-<ver>-x86_64.AppImage` | `chmod +x` 后直接运行；删除文件即卸载 |
| `Oxizip-<ver>-linux-x86_64.tar.gz` | 解压其中的 `usr/` 树 |

系统包名小写 `oxizip`；分发文件名统一 `Oxizip` 前缀。RPM 按 NEVRA 保留 `-1` Release 段。包内含 `/usr/bin/oxizip`、`oxizip-cli`、`oxizip.desktop`、文档、图标与 `/usr/share/oxizip/locales/`。RAR / WIM 依赖系统 `unrar`、`wimlib-imagex`（deb 中为 Recommends）。

本地打包（需 ImageMagick 才能生成 AppImage 所需 PNG）：

```bash
chmod +x scripts/package-linux.sh
./scripts/package-linux.sh
```

离线构建可设 `OXIZIP_FETCH_APPIMAGE_TOOLS=0`，并自行准备 `appimagetool` / `linuxdeploy`。

### CI 发版

打标签 **`vX.Y.Z`**（例 `v0.1.8`；`Cargo.toml` 只写 `0.1.8`）或手动跑 [Package](.github/workflows/package.yml)。仅当 Setup、deb、rpm、AppImage、tar.gz **全部齐全** 才创建 GitHub Release；缺任一产物则失败且不发 Release。清单见 [AGENTS.md](AGENTS.md)。

---

## 语言与字体

- CLI / GUI：**简体中文** 与 **English**（`--lang zh-cn|en`；GUI 内可切换）
- 语言文件：`crates/oxizip-i18n/locales/*.toml`；运行时还会合并  
  - 可执行文件旁 `locales/`  
  - Linux：`/usr/share/oxizip/locales/`  
  - 用户目录：`%APPDATA%\Oxizip\locales\` 或 `~/.config/oxizip/locales/`
- Windows GUI：默认「微软雅黑 UI」；Release 构建不弹控制台
- Windows CLI：控制台 UTF-8，减轻中文乱码
- Linux GUI：建议安装 CJK 字体（如 `fonts-noto-cjk`）

---

## 运行时外部工具

Oxizip **不修改、不链接**下列工具的源码或库；仅在需要时通过进程调用。

| 工具 | 用途 | Windows 查找顺序 | 其他平台 |
|---|---|---|---|
| **UnRAR** | RAR 仅解压 | `OXIZIP_UNRAR` → 同目录 → **`lib/unrar/UnRAR.exe`** → `PATH` | `OXIZIP_UNRAR` → 系统 `unrar` |
| **wimlib-imagex** | WIM 仅解压 | `OXIZIP_WIMLIB` → 同目录 → **`lib/wimlib/`** → `PATH` | `OXIZIP_WIMLIB` → 系统 `wimlib-imagex` 等 |

Windows 安装包捆绑仓库 [`lib/unrar/`](lib/unrar/) 与 [`lib/wimlib/`](lib/wimlib/) 中的官方未修改分发物及许可文本。缺失时返回可本地化错误。

**ZPAQ** 不 spawn 外部 exe：源码在 [`lib/zpaq/`](lib/zpaq/)，由 `oxizip-formats` 以 `-DOXIZIP_EMBEDDED` 编译；允许的改动仅限嵌入入口宏。发布请保留 `lib/zpaq/COPYING`。

---

## 开发

要求：Rust 工具链（`rustc` / `cargo`）。协作约定见 [AGENTS.md](AGENTS.md)。

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p oxizip-cli -- --help    # → oxizip-cli(.exe)
cargo run -p oxizip-gui              # → oxizip(.exe)
```

二进制：**GUI = `oxizip`**，**CLI = `oxizip-cli`**。

```bash
cargo run -p oxizip-cli -- c out.zip ./some_file
cargo run -p oxizip-cli -- c -f 7z -p secret -m lzma2 out.7z ./some_file
cargo run -p oxizip-cli -- c -f zipx -m zstd --volume-size 1048576 out.zipx ./big_dir
cargo run -p oxizip-cli -- l out.zip --password secret
cargo run -p oxizip-cli -- x out.7z.001 -o ./out_dir --password secret
cargo run -p oxizip-cli -- repair damaged.zip -o repaired.zip
cargo run -p oxizip-cli -- shell install
```

### 仓库结构

```
crates/
  oxizip-core/       # 进度、取消、错误、路径安全
  oxizip-formats/    # 格式探测与读写
  oxizip-engine/     # 统一 Facade
  oxizip-cli/        # 命令行
  oxizip-gui/        # iced 界面
  oxizip-i18n/       # 文案（locales/*.toml）
  oxizip-setup/      # Windows 安装 / 卸载 UI
  oxizip-sfx-stub/   # 自解压 stub
  oxizip-shellext/   # Windows Explorer 扩展（仅拉起 CLI）
lib/                 # unrar · wimlib（Windows 分发）· zpaq（嵌入）
packaging/           # desktop、Sparse 清单、deb 脚本等
samples/             # 自造测试样本
scripts/             # package-windows.ps1 · package-linux.sh
```

依赖方向：`cli / gui → engine → formats → core`（禁止反向）。`reference/` 仅本地参考（gitignore），禁止纳入构建。

---

## 许可

Oxizip 以 **GNU Affero General Public License v3.0 only（AGPL-3.0-only）** 发布，全文见 [LICENSE](LICENSE)。下表为主要依赖 / 外部工具自身许可证，不改变 Oxizip 的 AGPL 义务。完整传递依赖以 `Cargo.lock` 与各 crate 元数据为准；GUI「依赖与协议」页亦有摘要。

### 嵌入 / 链接

| 组件 | 用途 | 许可证 |
|---|---|---|
| ZPAQ（`lib/zpaq`） | `.zpaq` 读写 | Unlicense；内嵌 libdivsufsort-lite 为 MIT |
| cab / lzxd | Cabinet | MIT / MIT OR Apache-2.0 |
| unalz / unegg / libazo | ALZ / EGG | MIT |
| sevenz-turbo | 7z | Apache-2.0 |
| zip / tar / flate2 / bzip2 / lzma-rs | 归档与编解码 | MIT 或 MIT OR Apache-2.0 |
| zstd | Zstandard | BSD-3-Clause |
| iced / rfd / open / clap / thiserror 等 | GUI / CLI | MIT 或 MIT OR Apache-2.0 |

### 外部黑盒 CLI

| 工具 | 用途 | 许可证要点 |
|---|---|---|
| UnRAR（Alexander Roshal） | RAR 解压 | Freeware；可免费分发；禁止借此逆向重建专有 RAR **压缩**算法；AS IS |
| wimlib / wimlib-imagex | WIM 解压 | GPLv3+（libwim 在无冲突时可 LGPLv2.1+）；仅 spawn，不链接；捆绑分发须履行其义务 |

### 不纳入构建

- `reference/` 本地参考材料（gitignore）
- `third_party/unrar/`、`third_party/wimlib/` 下临时二进制 / SDK（gitignore；不链接 DLL）
- 正式内容在 [`lib/`](lib/)：`unrar/`、`wimlib/`、`zpaq/`

### 发布前清单

1. 附带 `LICENSE`（AGPL-3.0）；网络提供修改版服务时按 AGPL 提供对应完整源码  
2. 保留 `lib/zpaq/COPYING`  
3. 保留 `lib/unrar/`、`lib/wimlib/` 官方许可文本；wimlib 源码见 [wimlib.net](https://wimlib.net/downloads/)  
4. 不分发 `ark*.dll` 或未获授权专有库  
