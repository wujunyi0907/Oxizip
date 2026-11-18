# ZPAQ（进程内嵌入官方源码）

Oxizip **不**调用外部 `zpaq.exe`。处理 `.zpaq` 时链接并调用官方 `third_party/zpaq`（`libzpaq` + journaling 逻辑），通过 `oxizip_zpaq_run` 走与官方 CLI 相同的 `a` / `l` / `x` 语义。

## 约定

1. 仅支持 **64 位** 目标（与 Oxizip 整体一致）。
2. 源码保持官方原版；仅允许为嵌入增加 `OXIZIP_EMBEDDED` 入口（`oxizip_zpaq_run`），不改压缩算法。
3. 许可：Unlicense / public domain（见 `zpaq/COPYING`）；内嵌 divsufsort 为 MIT。
4. 树内只保留嵌入所需源码与许可文件，**不**存放官方预编译 exe。

## 树内文件

| 文件 | 用途 |
|---|---|
| `libzpaq.h` / `libzpaq.cpp` | 官方库 |
| `zpaq.cpp` | 官方 CLI 逻辑（`-DOXIZIP_EMBEDDED`） |
| `COPYING` / `readme.txt` | 许可与上游说明 |

## 构建

`oxizip-formats` 的 `build.rs` 用 `cc` 编译：

- `third_party/zpaq/libzpaq.cpp`
- `third_party/zpaq/zpaq.cpp`（`-DOXIZIP_EMBEDDED`）
- `crates/oxizip-formats/native/zpaq/oxizip_zpaq_capture.cpp`

Linux/macOS 定义 `unix` 并链接 `pthread`；Windows 链接 `advapi32` 等。
