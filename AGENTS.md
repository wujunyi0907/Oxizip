# Oxizip — Agent 指南

面向在本仓库工作的人类与 AI Agent。实现前先读本文与 `.cursor/rules/`。

## 项目是什么

Oxizip 是用 Rust **干净室**实现的压缩/解压工具（产品形态参考 Bandizip，**不**复制其代码、资源或专有库）。

已锁定产品决策：

| 项 | 决策 |
|---|---|
| GUI | `iced` |
| 预览 | 不做应用内预览；用系统默认程序打开 |
| RAR | **仅解压**；Windows 用官方 UnRAR CLI；其他平台用系统 `unrar`；禁止自研/修改 RAR |
| ZPAQ | 进程内嵌入官方 `third_party/zpaq`（`OXIZIP_EMBEDDED`），不依赖外部 exe |
| 架构 | **仅 64 位** |
| 参考目录 | `reference/Bandizip/`、`reference/BANDIZIP-SETUP-BETA-2.EXE` 仅作行为参考（已 gitignore），**禁止**纳入构建或拷贝进 `crates/` / `samples/` |

## 仓库布局（目标）

```
crates/
  oxizip-core/      # Progress / Cancel / Error / 路径安全
  oxizip-formats/   # sniff + 各格式 Reader/Writer
  oxizip-engine/    # 统一 Facade：detect/list/extract/create/test
  oxizip-cli/       # clap CLI
  oxizip-gui/       # iced GUI
  oxizip-i18n/      # 自有文案
third_party/        # 仅说明文档；官方 UnRAR 二进制若分发则原样放置
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

1. **干净室**：不嵌入 `ark*.dll`，不复制 Bandizip 皮肤/语言包/图标/授权或广告逻辑。
2. **路径安全**：一切解压落地路径必须经 `oxizip-core` 的防穿越校验（zip-slip）。
3. **进度与取消**：长时间作业接受 `Progress` + 取消令牌；GUI/CLI 不得阻塞到无法取消。
4. **格式逻辑只住在 formats/engine**：CLI/GUI 不得直接解析压缩格式。
5. **RAR**：只通过 UnRAR CLI 黑盒（Windows 官方二进制 / 其他平台系统安装）；无 create/update RAR；缺失时返回可本地化错误。
5b. **ZPAQ**：嵌入官方源码，不 spawn `zpaq.exe`；允许的源码改动仅限嵌入入口宏。
6. **打开文件**：解到受控临时目录后调用系统关联打开，不做内嵌预览控件。
7. **中文沟通**：面向用户的说明、提交说明按仓库约定；代码标识符用英文。

## 不要做

- 不要为了“快”把格式解析写进 `oxizip-gui` / `oxizip-cli`
- 不要 vendoring 并修改 unrar/RAR 源码
- 不要从 `reference/Bandizip/` 复制资源到本仓库发布路径
- 不要引入 WebView 广告、授权服务器校验、应用内文档/图片预览器
- 不要在未要求时添加无关 markdown / 扩大范围

## 编码规范入口

- 详细风格与 API 约定：[docs/CODING_STANDARDS.md](docs/CODING_STANDARDS.md)
- Cursor 强制提醒：`.cursor/rules/*.mdc`
- 格式化：`rustfmt.toml`；静态检查：`clippy.toml`（`cargo fmt` / `cargo clippy`）

## 工作方式

1. 改动前确认落在正确 crate，保持边界。
2. 新格式：在 `oxizip-formats` 实现 probe + reader/writer，再在 `oxizip-engine` 注册。
3. 测试样本放 `samples/`，只使用自造或明确许可的文件。
4. 优先小步提交；用户未要求时不 `git commit` / 不 `push`。
5. Windows 为主要目标平台；路径与进程调用考虑 Unicode。

## 相关文档

- 产品复刻计划：见 Cursor plan「Oxizip Rust Recreate」
- UnRAR 说明：`third_party/README-UnRAR.md`（落地后）
