# Oxizip — Agent 指南

面向在本仓库工作的人类与 AI Agent。实现前先读本文与 `.cursor/rules/`。用户可见说明与许可汇总见 [README.md](README.md)。

## 项目是什么

Oxizip 是用 Rust **干净室**实现的压缩/解压工具（不复制第三方专有代码、资源或库）。

已锁定产品决策：

| 项 | 决策 |
|---|---|
| GUI | `iced` |
| 预览 | 不做应用内预览；用系统默认程序打开 |
| RAR | **仅解压**；Windows 用官方 UnRAR CLI；其他平台用系统 `unrar`；禁止自研/修改 RAR |
| WIM | **仅解压**；官方 `wimlib-imagex` CLI；不链接 `libwim` |
| ZPAQ | 进程内嵌入官方 `third_party/zpaq`（`OXIZIP_EMBEDDED`），不依赖外部 exe |
| 架构 | **仅 64 位** |
| 应用 ID | `cn.wujunyi.oxizip`（反向域名：`wujunyi.cn` + 软件名；显示名仍为 Oxizip） |
| 安装器 | Windows：自研 `oxizip-setup`（iced），见 `scripts/package-windows.ps1`；Linux：deb/rpm（`/usr`）与 AppImage，见 `scripts/package-linux.sh`；CI 打包见 `.github/workflows/package.yml`；禁止嵌入第三方安装器资源 |
| 参考目录 | `reference/` 仅本地参考（已 gitignore），**禁止**纳入构建或拷贝进 `crates/` / `samples/` |

## 仓库布局

```
crates/
  oxizip-core/      # Progress / Cancel / Error / 路径安全
  oxizip-formats/   # sniff + 各格式 Reader/Writer
  oxizip-engine/    # 统一 Facade：detect/list/extract/create/test/…
  oxizip-cli/       # clap CLI
  oxizip-gui/       # iced GUI
  oxizip-i18n/      # 自有文案
  oxizip-sfx-stub/  # 自解压 stub
third_party/        # ZPAQ 源码；可选 UnRAR/wimlib 二进制（gitignore）
samples/            # 自造测试样本
```

依赖方向（禁止反向依赖）：

```
oxizip-cli / oxizip-gui
        ↓
  oxizip-engine
        ↓
  oxizip-formats → oxizip-core
  oxizip-i18n    →（可被 cli/gui 使用；不得依赖 engine）
```

## 必须遵守

1. **干净室**：不嵌入 `ark*.dll`，不复制第三方专有皮肤/语言包/图标/授权或广告逻辑。
2. **路径安全**：一切解压落地路径必须经 `oxizip-core` 的防穿越校验（zip-slip）；拒绝 `..`、绝对路径、盘符/UNC 逃逸。
3. **进度与取消**：长时间作业接受 `Progress` + 取消令牌；GUI/CLI 不得阻塞到无法取消；取消后尽快停写盘。
4. **格式逻辑只住在 formats/engine**：CLI/GUI 不得直接解析压缩格式。
5. **RAR**：只通过 UnRAR CLI 黑盒；无 create/update RAR；缺失时返回可本地化错误。查找：`OXIZIP_UNRAR` → 同目录 → `third_party/unrar/` → PATH（非 Windows 用系统 `unrar`）。
5b. **ZPAQ**：嵌入官方源码，不 spawn `zpaq.exe`；允许的源码改动仅限嵌入入口宏。由 `oxizip-formats` `build.rs` 编译 `libzpaq.cpp` / `zpaq.cpp`（`-DOXIZIP_EMBEDDED`）与 `native/zpaq/oxizip_zpaq_capture.cpp`。
5c. **WIM**：只通过 wimlib CLI；查找：`OXIZIP_WIMLIB` → 同目录 → `third_party/wimlib/` → PATH。
6. **打开文件**：解到受控临时目录后调用系统关联打开，不做内嵌预览控件。
7. **中文沟通**：面向用户的说明、提交说明按仓库约定；代码标识符用英文。

## 变更范围

- **只改完成需求所必需的文件**；不顺手重构无关代码、不清理既有遗留、不扩大范围或堆砌文档。
- **改前 / 改后**都应做相关构建与检查：至少对受影响 crate 跑通编译；提交前尽量 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、相关 `cargo test`。
- 注意源码编码（UTF-8）、CLI 控制台输出、GUI 交互与进度/取消是否正常。
- **GUI / 安装器 UI** 改动：验证命令与视觉核对范围写入提交说明或 PR（有条件时附截图或录屏）。

## 提交纪律

- **未获用户明确授权时**，不自动执行 `git add` / `git commit` / `git push` / 打标签。
- **禁止提交**：密钥与令牌、个人本地配置、`target/`、`dist/`、本地 `reference/`、`third_party/unrar/` / `third_party/wimlib/` 下的二进制 / SDK，以及其他生成物或缓存目录。
- 提交说明与 PR 格式见下文「提交说明」「合并请求」；发版见「发版 / 打标签」。

## 不要做

- 不要为了“快”把格式解析写进 `oxizip-gui` / `oxizip-cli`
- 不要 vendoring 并修改 unrar/RAR 源码；不要实现 RAR 压缩比特流
- 不要从 `reference/` 复制资源到本仓库发布路径
- 不要引入 WebView 广告、授权服务器校验、应用内文档/图片预览器
- 不要在未要求时添加无关 markdown / 扩大范围
- 不要在未授权时 `git add` / `commit` / `push`
- 不要在未更新 `Cargo.toml` 版本与 [CHANGELOG.md](CHANGELOG.md) 时打发布标签；发布标签必须是 `vX.Y.Z`（如 `v1.0.1`），禁止只有数字的标签名

## 编码规范

### 工具链

- Rust Edition：**2021**（全 workspace 统一，见根 `Cargo.toml`）
- 提交前本地至少：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、相关 `cargo test`
- 格式化 / 静态检查：`rustfmt.toml`、`clippy.toml`；Cursor 强制提醒：`.cursor/rules/*.mdc`

### 命名

| 种类 | 约定 | 示例 |
|---|---|---|
| Crate | `oxizip-*` kebab | `oxizip-engine` |
| 类型 / Trait | UpperCamelCase | `ArchiveReader`, `FormatId` |
| 函数 / 变量 | snake_case | `extract_all` |
| 常量 | SCREAMING_SNAKE | `MAX_HEADER_SNIFF` |
| 错误枚举 | `*Error`，`thiserror` 优先 | `EngineError` |

- 公共 API 文档注释用英文；用户可见字符串走 `oxizip-i18n`（zh-CN / en）。

### 错误、线程、依赖

- 库内禁止无故 `unwrap`/`expect`（测试与确凿不变式除外，`expect` 须写明理由）。
- 用户可感知错误必须可映射到 i18n key；禁止只抛裸英文给 GUI。
- CLI 退出码：成功 0；取消与失败在 `oxizip-cli` 集中定义。
- 引擎以 **同步 API + 可取消** 为主；GUI 在 worker 线程调用，经 channel 回传进度；禁止在 iced UI 线程做重 CPU / 大文件解压。
- 新增依赖须兼容 AGPL 策略，workspace 统一 `workspace.dependencies`；少用 git 依赖；禁止迫使修改 unrar 源码的 crate。

### 格式插件

在 `oxizip-formats` 提供：`sniff` → `ArchiveReader`（list/extract/可选 test）→ 可选 `ArchiveWriter`；再在 `oxizip-engine` 注册。单元测试 + `samples/` 自造样本。RAR/WIM 模块名体现 CLI 后端，内部只允许 `Command` 调官方二进制。

### GUI / CLI

- 视图层不调用 formats；只发消息，由 update/worker 调 `oxizip-engine`。
- 文案全部经 i18n。
- CLI：`clap` derive；日志 `tracing`；`-v` 增加细节；与 GUI 共享 engine。

### 测试

- 解压/打包集成测试放 `samples/`；覆盖 zip-slip 负例。
- RAR/WIM：环境无 CLI 时 skip，并断言缺失错误类型。
- 注释解释「为什么」；公开 API 写 `///`；优先避免 `unsafe`。

### 提交说明（Conventional Commits + 中文）

采用简洁的 Conventional Commit 前缀，**后接中文说明**；一次提交只含一个明确主题。

| 前缀 | 用途 | 示例 |
|---|---|---|
| `feat:` | 新功能 | `feat: 新增 iced 自定义安装与卸载界面` |
| `fix:` | 缺陷修复 | `fix: 修复 AppImage 插件自引用符号链接` |
| `refactor:` | 无行为变化的重构 | `refactor: 抽离壳关联图标注册逻辑` |
| `docs:` | 仅文档 | `docs: 补充发版标签 vX.Y.Z 说明` |
| `chore:` | 构建/CI/杂务 | `chore: 调整 Package 工作流触发条件` |
| `test:` | 仅测试 | `test: 补充 zip-slip 负例` |
| `perf:` | 性能 | `perf: 降低大文件解压进度回调频率` |

- 正文可另起一行写动机（为什么），避免只罗列文件名。
- UI 相关提交：说明验证命令与视觉核对范围（例：`cargo run -p oxizip-setup`、安装页按钮是否完整可见）。

### 合并请求（Pull Request）

PR / MR 描述应包含：

1. **用户可见变化**（做了什么、影响谁）
2. **验证命令**（fmt / clippy / test / 手动跑 GUI 或打包脚本等）
3. **目标平台**（如 Windows x64、Linux x86_64）
4. 视觉改动时附**截图或录屏**，并关联相关 issue（有则链接）

### 发版 / 打标签（必做清单）

**Git 标签格式（强制）**：必须是小写 **`v` + 语义化版本**，例如 `v1.0.1`、`v0.1.2`。

| 正确 | 错误 |
|---|---|
| `v1.0.1` | `1.0.1`（缺 `v` 前缀，CI / Package 不会按发布标签处理） |
| `v0.2.0` | `V0.2.0`、`ver-0.2.0`、`release-0.2.0` |

`Cargo.toml` 里的 `workspace.package.version` **只写数字**（如 `1.0.1`），**不要**写成 `v1.0.1`；标签名 = `v` + 该版本号。

打标签或触发正式发布前，**不要遗漏**下列项（Agent 与人类均须遵守）：

1. **版本号**：`Cargo.toml` 的 `version = "X.Y.Z"` 与标签 `vX.Y.Z` 对应（例如 toml=`1.0.1` ↔ tag=`v1.0.1`）。CI 会校验；不一致则失败。
2. **CHANGELOG.md**：把 `[Unreleased]` 中的条目移到新版本节（日期用当天），写清 Added / Changed / Fixed 等；更新文末 compare / tag 链接；不要空着就打标签。
3. **说明文档**：若安装方式、产物名、外部工具有变，同步 [README.md](README.md) / [README.en.md](README.en.md)。
4. **提交后再打标签**：上述改动先 commit 并推到 `main`，再例如：`git tag -a v1.0.1 -m "Oxizip 1.0.1"` → `git push origin v1.0.1`（Package 仅匹配 `v*` 标签或手动触发）。
5. **不要**：打纯数字标签（如 `1.0.1`）；不要在未更新 CHANGELOG / `Cargo.toml` 版本时打标签；不要用与 toml 不符的标签；不要指望只靠 GitHub 自动 Release Notes 代替 CHANGELOG。

## 工作方式

1. 改动前确认落在正确 crate，保持边界；只改需求所需文件。
2. 新格式：在 `oxizip-formats` 实现 probe + reader/writer，再在 `oxizip-engine` 注册。
3. 测试样本放 `samples/`，只使用自造或明确许可的文件。
4. 优先小步；**未获明确授权不** `git add` / `commit` / `push`。
5. Windows 为主要目标平台；路径与进程调用考虑 Unicode。
6. 用户要求发版 / 打标签时：先走「发版 / 打标签」清单；标签一律 `vX.Y.Z`（例 `v1.0.1`），再 push。
7. 用户要求提交时：使用 `feat:` / `fix:` 等前缀 + 中文主题；一次一个主题。

## 许可入口

- 项目：`LICENSE`（**AGPL-3.0-only**）
- 第三方与运行时工具：见 [README.md](README.md)「许可」与「运行时外部工具」
