# UnRAR（官方黑盒 CLI）

Oxizip **不包含、不修改**任何 RAR/unrar 源码，也不链接 UnRAR DLL。

## 规则

1. 仅通过官方 UnRAR CLI 做 **list / extract / test**。
2. 不实现 RAR 创建或更新。
3. 仅支持 **64 位** 系统。
4. 若随 Windows 安装包分发 UnRAR，必须使用官方原版 **64 位** CLI，并遵守其许可。

## 平台策略

| 平台 | 行为 |
|---|---|
| **Windows** | 官方二进制：`OXIZIP_UNRAR` → 程序同目录 `UnRAR.exe` → `third_party/unrar/` → `PATH` |
| **Linux / macOS 等** | 使用系统安装的 `unrar`（发行版包装或用户自装）；可用 `OXIZIP_UNRAR` 覆盖 |

缺失时返回可本地化错误（`UnrarMissing`）。

## 说明

`third_party/UnrarDLL/` 若存在，仅为参考/SDK，**Oxizip 不链接 DLL**。Windows 运行需要的是官方 `UnRAR.exe` CLI。
