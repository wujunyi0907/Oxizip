# Oxizip

用 Rust 编写的跨格式压缩 / 解压工具：共享引擎，提供命令行与桌面界面。

## 特性（规划）

- **引擎统一**：列表、解压、创建、校验走同一套 API，CLI 与 GUI 共用
- **命令行**：`clap` 子命令（list / extract / create / test）
- **桌面端**：`iced`；条目通过系统默认程序打开（无应用内预览）
- **路径安全**：解压防目录穿越
- **进度与取消**：长时间任务可报告进度并支持取消
- **RAR**：仅支持解压，通过官方未修改的 UnRAR 命令行完成（不自研、不修改任何 RAR 相关源码）

### 创建（压缩）格式

`zip` · `7z` · `zipx` · `exe`（自解压）· `tar` · `tgz` · `lzh` · `iso` · `gz` · `xz` · `zpaq`（进程内嵌入官方源码）

仅支持 **64 位**。RAR 解压：Windows 用官方 UnRAR CLI；其他平台用系统 `unrar`。

CLI：`oxizip c -f 7z out.7z ./files`

### 解压 / 列表

ZIP / ZIPX / EXE(SFX) / TAR 族 / GZ·XZ·ZST·BZ2 / 7Z / LZH / ISO / ZPAQ；RAR 仅解压（官方 UnRAR）。

## 仓库结构（目标）

```
crates/
  oxizip-core/       # 进度、取消、错误、路径安全
  oxizip-formats/    # 格式探测与读写
  oxizip-engine/     # 统一 Facade
  oxizip-cli/        # 命令行
  oxizip-gui/        # iced 界面
  oxizip-i18n/       # 文案
third_party/         # 第三方说明（如 UnRAR 分发约定）
samples/             # 自造测试样本
```

## 开发

要求：已安装 Rust（`rustc` / `cargo`）。

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p oxizip-cli -- --help
cargo run -p oxizip-gui
```

CLI 示例：

```bash
cargo run -p oxizip-cli -- c out.zip ./some_file
cargo run -p oxizip-cli -- c -f 7z -p secret -m lzma2 out.7z ./some_file
cargo run -p oxizip-cli -- c -f zipx -m zstd --volume-size 1048576 out.zipx ./big_dir
cargo run -p oxizip-cli -- l out.zip --password secret
cargo run -p oxizip-cli -- x out.7z.001 -o ./out_dir --password secret
```

编码与协作约定见 [AGENTS.md](AGENTS.md)、[docs/CODING_STANDARDS.md](docs/CODING_STANDARDS.md)。

## 许可

项目许可待 workspace 落地后在此补充。使用官方 UnRAR / ZPAQ 时须另行遵守其许可条款，见 [third_party/README-UnRAR.md](third_party/README-UnRAR.md)、[third_party/README-ZPAQ.md](third_party/README-ZPAQ.md)。
