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
| 安装器 | Windows：自研 `oxizip-setup`（iced），见 `scripts/package-windows.ps1`；Linux：deb/rpm（`/usr`）与 AppImage，见 `scripts/package-linux.sh`；禁止嵌入第三方安装器资源 |
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

## 不要做

- 不要为了“快”把格式解析写进 `oxizip-gui` / `oxizip-cli`
- 不要 vendoring 并修改 unrar/RAR 源码；不要实现 RAR 压缩比特流
- 不要从 `reference/` 复制资源到本仓库发布路径
- 不要引入 WebView 广告、授权服务器校验、应用内文档/图片预览器
- 不要在未要求时添加无关 markdown / 扩大范围
- 用户未要求时不 `git commit` / 不 `push`

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

### 测试与 Git

- 解压/打包集成测试放 `samples/`；覆盖 zip-slip 负例。
- RAR/WIM：环境无 CLI 时 skip，并断言缺失错误类型。
- 不提交密钥、本地 UnRAR/wimlib 二进制、整个 `reference/`。
- 提交说明写清动机；团队默认中文简述。
- 注释解释「为什么」；公开 API 写 `///`；优先避免 `unsafe`。

## 工作方式

1. 改动前确认落在正确 crate，保持边界。
2. 新格式：在 `oxizip-formats` 实现 probe + reader/writer，再在 `oxizip-engine` 注册。
3. 测试样本放 `samples/`，只使用自造或明确许可的文件。
4. 优先小步；用户未要求时不 commit/push。
5. Windows 为主要目标平台；路径与进程调用考虑 Unicode。

## 许可入口

- 项目：`LICENSE`（**AGPL-3.0-only**）
- 第三方与运行时工具：见 [README.md](README.md)「许可」与「运行时外部工具」
