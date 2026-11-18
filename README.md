# Oxizip

[English](README.en.md)

用 Rust **干净室**实现的跨格式压缩 / 解压工具：共享引擎，提供命令行与桌面界面。

## 特性

- **引擎统一**：列表、解压、创建、校验、修复、密码恢复走同一套 API；CLI 与 GUI 共用
- **命令行**：`clap` 子命令（list / extract / create / test / detect / add / delete / rename / checksum / repair / recover-password / shell）
- **桌面端**：`iced`；条目通过系统默认程序打开（无应用内预览）；自有图标见 `crates/oxizip-gui/assets/icons`
- **路径安全**：解压防目录穿越（zip-slip）
- **进度与取消**：长时间任务可报告进度并支持取消
- **壳集成**：`oxizip-cli shell install`（Windows 右键菜单 / Linux 桌面动作）
- **架构**：仅 **64 位**

### 创建（压缩）

`zip` · `7z` · `zipx` · `exe`（自解压）· `tar` · `tgz` · `lzh` · `iso` · `gz` · `xz` · `zpaq`（进程内嵌入官方源码）

### 解压 / 列表

ZIP / ZIPX / EXE(SFX) / TAR 族 / GZ·XZ·ZST·BZ2 / 7Z / LZH / ISO / ZPAQ / CAB / ALZ / EGG / WIM（wimlib CLI）/ RAR（官方 UnRAR CLI，仅解压）

### 其他能力

- **修复**（best-effort）：损坏的 ZIP / 7z
- **密码恢复**（ZIP / 7z）：词典 + 可选短爆破
- **校验和**：CRC32 / SHA-256；Windows 可写 Zone.Identifier

## 仓库结构

```
crates/
  oxizip-core/       # 进度、取消、错误、路径安全
  oxizip-formats/    # 格式探测与读写
  oxizip-engine/     # 统一 Facade
  oxizip-cli/        # 命令行
  oxizip-gui/        # iced 界面
  oxizip-i18n/       # 文案
  oxizip-sfx-stub/   # 自解压 stub
third_party/         # 嵌入源码（如 ZPAQ）与可选本地 CLI 二进制（gitignore）
samples/             # 自造测试样本
```

依赖方向：`cli/gui → engine → formats → core`（禁止反向）。

## 开发

要求：已安装 Rust（`rustc` / `cargo`）。

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p oxizip-cli -- --help          # → oxizip-cli(.exe)
cargo run -p oxizip-gui                    # → oxizip(.exe)
```

二进制命名：**GUI = `oxizip`**，**CLI = `oxizip-cli`**。

CLI 示例：

```bash
cargo run -p oxizip-cli -- c out.zip ./some_file
cargo run -p oxizip-cli -- c -f 7z -p secret -m lzma2 out.7z ./some_file
cargo run -p oxizip-cli -- c -f zipx -m zstd --volume-size 1048576 out.zipx ./big_dir
cargo run -p oxizip-cli -- l out.zip --password secret
cargo run -p oxizip-cli -- x out.7z.001 -o ./out_dir --password secret
cargo run -p oxizip-cli -- repair damaged.zip -o repaired.zip
cargo run -p oxizip-cli -- shell install
```

协作与编码约定见 [AGENTS.md](AGENTS.md)。

## 界面语言与字体

- CLI / GUI 均支持 **简体中文** 与 **English**（`--lang zh-cn|en`；GUI 内可切换）。
- Windows GUI：默认使用「微软雅黑 UI」；**Release** 构建不弹出控制台窗口。
- Windows CLI：启动时将控制台代码页设为 UTF-8，避免中文乱码。
- Linux GUI：依赖系统 CJK 字体（如 `fonts-noto-cjk`）。

## 打包安装包

GitHub Actions：打 `v*` 标签或手动运行 [Package](.github/workflows/package.yml) 会产出 Windows Setup 与 Linux deb/rpm/AppImage。打 `v*` 标签时，**仅当 Setup / deb / rpm / AppImage / tar.gz 全部齐全**才创建 GitHub Release 并上传；缺任一产物则失败且不发 Release（不在每次推送 `main` 时打包，避免与打标签重复跑两遍）。

### Windows（自定义安装器 `Oxizip-Setup-<version>-x64.exe`）

自研 iced 安装 / 卸载界面（路径选择、许可确认、可选文件关联）：

```powershell
.\scripts\package-windows.ps1
```

产物：`dist\Oxizip-Setup-<version>-x64.exe`（版本取自根目录 `Cargo.toml` 的 `workspace.package.version`）。

- 默认安装到 `C:\Program Files\Oxizip`（会请求管理员权限）
- 内嵌 `oxizip.exe`（GUI）与 `oxizip-cli.exe`（CLI）及格式图标
- 安装完成后可启动程序并引导关联；卸载程序为安装目录下的 `Uninstall.exe`（同一套自定义界面）
- 请分发 Setup；不要把单独的 `oxizip.exe` 当作安装包

### Linux（`.deb` / `.rpm` / AppImage + tar.gz）

在 Linux 本机或容器中（64 位）：

```bash
chmod +x scripts/package-linux.sh
./scripts/package-linux.sh
```

| 产物 | 工具 | 安装 / 卸载 |
|---|---|---|
| `dist/Oxizip_<ver>_<arch>.deb` | `dpkg-deb` | `sudo dpkg -i …` / `sudo apt remove oxizip` |
| `dist/Oxizip-<ver>-1.<arch>.rpm` | `rpmbuild` 或 `fpm` | `sudo rpm -Uvh …` / `sudo rpm -e oxizip` |
| `dist/Oxizip-<ver>-<arch>.AppImage` | `linuxdeploy` + `appimagetool`（缺省自动下载） | `chmod +x` 后直接运行；删除文件即卸载 |
| `dist/Oxizip-<ver>-linux-<arch>.tar.gz` | tar | 手动解压 `usr/` 树 |

分发文件名统一 `Oxizip` 首字母大写。deb/AppImage/tar/exe 没有 Release 段；**RPM 按 NEVRA 保留** `Oxizip-<ver>-1.<arch>.rpm`（`-1` 为 Release 构建轮次）。系统包名仍是小写 `oxizip`（`apt`/`rpm` 查询用）。

包内布局：`/usr/bin/oxizip`、`/usr/bin/oxizip-cli`、`/usr/share/applications/oxizip.desktop`、文档与图标（有 ImageMagick 时从 ICO 生成；**AppImage 需要 PNG 图标**）。缺对应工具时会跳过该格式。离线构建可设 `OXIZIP_FETCH_APPIMAGE_TOOLS=0` 并自行把 `appimagetool` / `linuxdeploy` 放进 `PATH`。

## 运行时外部工具

Oxizip **不修改、不链接**下列工具的源码/库；仅在需要时通过 `Command` 调用。

### UnRAR（RAR 仅解压）

| 平台 | 查找顺序 |
|---|---|
| Windows | `OXIZIP_UNRAR` → 程序同目录 `UnRAR.exe` → `third_party/unrar/` → `PATH` |
| 其他 | `OXIZIP_UNRAR` → 系统 `unrar` |

缺失时返回可本地化错误。若随包分发：须用官方未修改 64 位 CLI，并保留其许可文本。

### wimlib（WIM 仅解压）

| 平台 | 查找顺序 |
|---|---|
| Windows | `OXIZIP_WIMLIB` → 同目录 `wimlib-imagex.exe` → `third_party/wimlib/` → `PATH` |
| 其他 | `OXIZIP_WIMLIB` → 系统 `wimlib-imagex` / `wimdir`+`wimextract` |

### ZPAQ（进程内嵌入）

不调用外部 `zpaq.exe`。源码在 `third_party/zpaq/`，由 `oxizip-formats` 的 `build.rs` 以 `-DOXIZIP_EMBEDDED` 编译；允许的源码改动仅限嵌入入口宏。发布时请保留 `third_party/zpaq/COPYING`。

## 许可

Oxizip 以 **GNU Affero General Public License v3.0 only（AGPL-3.0-only）** 发布，全文见 [LICENSE](LICENSE)。下表为主要依赖/外部工具自身许可证，不改变 Oxizip 的 AGPL 义务。完整传递依赖以 `Cargo.lock` 与各 crate 元数据为准；GUI「依赖与协议」页亦有摘要。

### 嵌入 / 链接

| 组件 | 用途 | 许可证 |
|---|---|---|
| ZPAQ（`third_party/zpaq`） | `.zpaq` 读写 | Unlicense；内嵌 libdivsufsort-lite 为 MIT |
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

- `reference/` 下本地参考材料（gitignore，不纳入构建）
- `third_party/unrar/`、`third_party/wimlib/` 下本地 `*.exe` / SDK（可选，gitignore；不链接 DLL）

### 发布前清单

1. 附带 `LICENSE`（AGPL-3.0）；网络提供修改版服务时按 AGPL 提供对应完整源码
2. 保留 `third_party/zpaq/COPYING`
3. 若捆绑 UnRAR / wimlib：附其官方许可并履行相应义务
4. 不分发 `ark*.dll` 或未获授权专有库
