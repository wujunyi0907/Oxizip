# ZPAQ（进程内嵌入官方源码）

Oxizip **不**调用外部 `zpaq.exe`。创建 `.zpaq` 时链接并调用官方 `third_party/zpaq`（libzpaq + zpaq journaling 逻辑），通过 `oxizip_zpaq_run` 走与官方 CLI 相同的 `a` /（后续）`l`/`x` 语义。

## 约定

1. 仅支持 **64 位** 目标（与 Oxizip 整体一致）。
2. 源码保持官方原版；仅允许为嵌入增加 `OXIZIP_EMBEDDED` 入口（`oxizip_zpaq_run`），不改压缩算法。
3. 许可：Unlicense / public domain（见 `zpaq/COPYING`）；内嵌 divsufsort 为 MIT。
4. 树内 `zpaq.exe` / `zpaq64.exe` 仅作对照测试，**运行时不依赖**。

## 构建

`oxizip-formats` 的 `build.rs` 用 `cc` 编译：

- `third_party/zpaq/libzpaq.cpp`
- `third_party/zpaq/zpaq.cpp`（`-DOXIZIP_EMBEDDED`）

Linux/macOS 定义 `unix` 并链接 `pthread`；Windows 链接 `advapi32` 等。
