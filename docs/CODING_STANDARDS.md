# Oxizip 编码规范

本文是人类与 Agent 的详细约定。摘要见根目录 [AGENTS.md](../AGENTS.md)。

## 1. 语言与工具链

- 语言：Rust（Edition **2024**；若工具链尚未支持则用 **2021**，全 workspace 统一，禁止混用）
- 包管理：Cargo workspace；前端若另有工具，优先 **pnpm**（本仓库主体为 Rust）
- 提交前本地至少：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、相关 `cargo test`

## 2. 命名

| 种类 | 约定 | 示例 |
|---|---|---|
| Crate | `oxizip-*` kebab | `oxizip-engine` |
| 类型 / Trait | UpperCamelCase | `ArchiveReader`, `FormatId` |
| 函数 / 变量 | snake_case | `extract_all`, `entry_count` |
| 常量 | SCREAMING_SNAKE | `MAX_HEADER_SNIFF` |
| 错误枚举 | `*Error`，`thiserror` 优先 | `EngineError` |
| 特征实现模块 | 按格式分文件 | `formats/zip.rs`, `formats/rar_unrar.rs` |

- 公共 API 文档注释用英文；用户可见字符串走 `oxizip-i18n`，默认提供 **zh-CN** 与 **en**。

## 3. 错误处理

- 库代码：`Result<T, E>`，禁止库内 `unwrap`/`expect`（测试与确凿不变式除外，`expect` 必须写明理由）。
- 使用 `thiserror` 定义分层错误；用 `#[from]` / `source` 保留因果。
- 用户可感知错误必须可映射到 i18n key（例如 `error.unrar_missing`），禁止只抛裸英文给 GUI。
- CLI 退出码：成功 0；用户取消单独码；其它失败非 0（在 `oxizip-cli` 集中定义，勿魔法数字散落）。

```rust
// 禁止
let f = File::open(path).unwrap();

// 提倡
let f = File::open(path).map_err(EngineError::Io)?;
```

## 4. 异步与线程

- 引擎默认以 **同步 API + 可取消** 为主；GUI 在 worker 线程调用，经 channel 回传进度。
- 若引入 `async`，限于 IO 边界 crate，不把整条解压栈无故 async 化。
- 禁止在 iced UI 线程做重 CPU/大文件解压。

## 5. 进度与取消

```rust
pub trait Progress: Send {
    fn report(&self, update: ProgressUpdate);
}

// 每个可中断循环检查 cancel；触发后返回明确的 Canceled 错误
```

- `ProgressUpdate` 至少包含：已处理字节/总字节（可知时）、当前条目名、阶段（list/extract/create/test）。
- 取消必须尽快停止写盘；已写入的部分策略（保留/回滚临时文件）在 engine 层统一说明。

## 6. 路径与安全

- 所有归档内相对路径先规范化，再通过 `oxizip-core` 校验后拼接目标根目录。
- 拒绝：`..` 段、绝对路径、Windows 盘符与 UNC 逃逸、链接逃逸（按平台能力处理）。
- 临时解压打开文件：使用专用临时目录，进程退出或任务结束时清理；文件名避免冲突。

## 7. 格式插件约定

每个格式在 `oxizip-formats` 中应提供：

1. `sniff(&[u8], Option<&Path>) -> Option<FormatId>`
2. `ArchiveReader`（list / extract / 可选 test）
3. 若支持创建：`ArchiveWriter`
4. 单元测试 + `samples/` 中自造样本

RAR 特例：模块名体现后端（如 `rar_unrar.rs`），内部只允许 `std::process::Command` 调官方 UnRAR，不出现 RAR 比特流解析实现。

## 8. iced GUI

- 状态与消息：`Application`/`Sandbox` 等价模式（跟进所用 iced 版本的推荐结构）。
- 视图层不调用 formats；只发消息，由 update/worker 调 `oxizip-engine`。
- “打开条目” = engine 解压到临时路径 + 系统打开；无内嵌预览组件。
- 文案全部经 i18n，禁止硬编码大段用户字符串在 UI 文件中。

## 9. CLI

- `clap` derive；子命令：`l`/`x`/`c`/`t`（可保留长名别名）。
- 日志用 `tracing`；默认对人友好摘要，`-v` 增加细节。
- 与 GUI 共享 engine，不复制解压逻辑。

## 10. 依赖与许可

- 新增依赖需兼容仓库许可策略，并在 README/NOTICE 可追溯。
- 禁止引入会迫使我们修改 unrar 源码的 crate；RAR 只走官方 CLI。
- 少用 `git` 依赖；锁定版本，workspace 统一 `workspace.dependencies`。

## 11. 测试

- 纯逻辑：单元测试与 crate 内测。
- 解压/打包：`samples/` 集成测试；覆盖 zip-slip 负例。
- RAR 测试：若环境无 UnRAR，标记 `ignore` 或运行时 skip，并断言“缺失时的错误类型”。

## 12. Git

- 仅在用户明确要求时 commit/push。
- 不提交：密钥、本地 UnRAR 许可密钥、整个 `reference/` 目录（已 gitignore）。
- 提交说明写清动机；中英文均可，团队默认中文简述。

## 13. 注释与文档

- 解释“为什么”，避免复述代码。
- 公开 trait/函数写 `///`。
- 不安全代码（若有）必须块注释说明不变式；优先避免 `unsafe`。
