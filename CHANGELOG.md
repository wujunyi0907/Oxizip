# Changelog

本文档记录 Oxizip 面向用户的版本变更，格式参考 [Keep a Changelog](https://keepachangelog.com/)。

版本号遵循 [Semantic Versioning](https://semver.org/)。发版 Git 标签格式为 **`v` + 版本号**（例：`v1.0.1`），`Cargo.toml` 中只写 `1.0.1`（无 `v`）。打标签前必须更新本文件，并将 `[Unreleased]` 归入新版本（见 [AGENTS.md](AGENTS.md)）。

## [Unreleased]

### Fixed

- 关联页：安装器 `--setup` 不再无视 `setup_done`；已完成/跳过或已是默认打开则进主界面
- Win11 Sparse 右键：补齐「解压并删除 / 解压对话框 / 用 Oxizip 打开」等项，并按 `shell_menu.json` 过滤显示
- 设置：主界面再拉起独立 `--settings` 进程；主窗保持可见但禁用（半透明遮罩），设置关闭后恢复

### Added

- GUI：Bandizip 风格独立设置窗口（`--settings`，左导航：常规 / 文件关联 / 上下文菜单 / 解压 / 压缩 / 查看 / 高级）；语言仍走「设置 → 语言」菜单二级
- 设置：关联与右键菜单开关可写回 `shell install`（含 `--menus-only` + `shell_menu.json` 过滤）
- 设置：重置默认、导出/导入 prefs、临时目录与高级偏好项（含 Zone.Identifier）

### Changed

- 设置确定写回 Prefs 并应用关联/菜单；`--settings` 进程关闭/Esc 丢弃草稿
- `--menus-only` 同步刷新已有 ProgID 的双击打开命令（上下文菜单偏好真正生效）
- 设置对照 Bandizip（不含安全/内置预览）：常规「修改后测试」、上下文菜单解压对话框/打开/并删除与多格式压缩、压缩级别与排除过滤/默认新建目录、查看「类型」列、高级回收站/起始页/临时尽快清理；UTF-8 ZIP 名与排除规则写入 CreateOptions
- 设置偏好已接入行为：双击动作 / 隐藏菜单图标、Zone.Identifier 开关、取消解压清理、智能解压单文件进新目录、临时目录、长路径、`[..]`/网格/单击打开、排除 Thumbs.db、单文件夹根目录、压缩后测试等

## [0.1.7] - 2026-11-21

### Fixed

- Linux：`kde_servicemenu` 去掉未使用的 `cli` 命名参数，修复 Package 编译失败
- `oxizip-cli`：`winresource` / `cc` 仅作 Windows build-dependencies，消除 Linux 上 unused dependency 警告

## [0.1.6] - 2026-11-21

### Added

- 自研 `oxizip-shellext` + Win11 Sparse Package（`IExplorerCommand` 级联，仅拉起 `oxizip-cli`）
- Sparse 注册/卸载脚本；安装包捆绑 DLL、已签名 Sparse MSIX 与发布者证书
- GUI 紧凑作业进度窗（`--job create|extract`，对齐 Bandizip：双进度条 / 用时 / 停止 / 完成后关闭）
- 右键压缩与解压（此处 / 智能 / 归档名目录）改为拉起 GUI 进度窗（经典动词与 Sparse）
- 解压遇同名文件时提示「覆盖」或「自动改名」（`name (1).ext`）
- Win11 Sparse 注册：信任自签 `CN=Oxizip`、AllowAllTrustedApps；安装器强制尝试注册现代右键菜单

### Changed

- Package CI：`checkout@v7`、`upload-artifact@v7`、`download-artifact@v8`（Node 24）
- 经典右键（「显示更多选项」）改回**扁平**解压/压缩项；级联仅出现在 Win11 现代菜单
- 不再注册经典 `ContextMenuHandlers`，避免双份「Oxizip」
- 安装时先 `shell install --menus-only` 注册右键；关联页仅管默认打开，文案不再提右键；已关联则跳过关联页进主界面

### Fixed

- CLI：`create --beside` 误把输入路径当成 ARCHIVE，导致右键「压缩为 ZIP/7z」闪退且无产物
- 卸载残留：对齐 Bandizip（`launch_in_temp` + `CloseApp explorer` + `DeleteFolderRec`/`SHFileOperation`），点「完成」后彻底删除安装目录
- 关联页说明去掉「ProgID / UserChoice / 可稍后」技术旁注
- Win11 现代右键：打包时预签名 Sparse MSIX（`oxizip-shellext.msix` + `Oxizip.cer`），安装时信任发布者再 `Add-AppxPackage`；失败回退开发者解锁 + 松散 `-Register`，并写入 `sparse-register.log`
- Win11 Sparse 注册：去掉安装过程中的 `regsvr32`（易被 AV 卡住导致安装器一直「处理中」）；`Add-AppxPackage` 与安装器侧均加超时，超时后仍完成安装
- Win11 现代菜单「Oxizip」父项：`GetIcon` 改为返回 `assets\icons\oxizip.ico`（此前 `E_NOTIMPL` 导致无图标）

## [0.1.5] - 2026-11-20

### Added

- Windows 安装包捆绑官方 `lib/unrar/` 与 `lib/wimlib/`（含 `libwim-15.dll` 及许可文本）

### Changed

- UnRAR / wimlib / ZPAQ 统一放在仓库 `lib/<name>/`；查找顺序增加 `<app>\lib\unrar|wimlib\`
- 主 GUI 与安装器统一为无边框圆角窗口（自绘外框 + 自定义标题栏；Win11 DWM 圆角）
- Package CI：`checkout` / artifact actions 升到 v5（消除 Node 20 弃用警告）

### Fixed

- Package Release：从 CHANGELOG 提取版本节时不再在标题行误退出（修复 v0.1.4 发版失败）

## [0.1.4] - 2026-11-20

### Added

- 多尺寸格式图标（对齐 Bandizip 的 7 档 ICO：含 8 位/32 位 BMP-DIB 与 AND 掩码）
- 安装器专用 `oxizip-setup.ico`（品牌 + UAC 盾）与独立 `oxizip-uac.ico`

### Changed

- 壳关联 DefaultIcon 改为裸 `.ico` 路径；安装产物不再打包 setup/UAC 专用图标

### Fixed

- 8 位色 ICO 圆角透明（AND 掩码写入修复）
- 16px 格式角标文字可读性
- GitHub Release 正文改为从 `CHANGELOG.md` 提取，避免自动生成说明为空

## [0.1.3] - 2026-11-20

### Added

- 安装时默认创建桌面快捷方式
- GUI 完整菜单栏（下拉对齐与 SVG 箭头）
- 首次安装文件关联界面支持按扩展名勾选注册
- 安装/卸载进度条

### Changed

- Windows 文件关联对齐经典 `UserChoice`（不写 `UserChoiceLatest`）；提权时同步 HKLM Classes
- 磁盘映像（iso / wim / esd / swm）保留在关联页，默认不勾选
- 开始菜单与卸载项改用 `oxizip.ico` 并刷新图标缓存
- 安装器启动时先请求 UAC，再进入安装界面
- 分发产物文件名统一为 `Oxizip` 前缀；RPM 仍按 NEVRA 保留 `Release`
- GitHub Release 仅在全部产物齐全时创建；Package 仅 `v*` 标签或手动触发
- 完善变更范围、提交纪律与跨平台规范（见 [AGENTS.md](AGENTS.md)）

### Fixed

- 拒绝 UAC 后不再进入安装界面
- 安装时避免公用桌面与用户桌面重复创建快捷方式
- Win11 / Linux 文件关联若干问题

## [0.1.2] - 2026-11-18

### Added

- 英文 README，与中文 README 互相链接

### Changed

- Package CI 改为仅 `v*` 标签 / `workflow_dispatch` 触发

## [0.1.1] - 2026-11-18

### Fixed

- Linux AppImage：修复 `linuxdeploy-plugin-appimage` 自引用符号链接导致构建失败；失败时回退 `appimagetool`

## [0.1.0] - 2026-11-18

### Added

- 自研 Windows 安装/卸载器 `oxizip-setup`（iced），替换 NSIS / cargo-packager
- Windows 壳关联、按格式图标、GUI 首次设置引导；不关联 `.exe`
- Linux 系统级 `.deb` / `.rpm`、AppImage 与 FHS tar.gz 打包脚本
- GitHub Actions Package 工作流：产出带版本号的 Setup / deb / rpm / AppImage，并在标签发版时上传 Release

### Changed

- 二进制命名：GUI = `oxizip`，CLI = `oxizip-cli`
- 应用 ID：`cn.wujunyi.oxizip`

[Unreleased]: https://github.com/wujunyi0907/Oxizip/compare/v0.1.7...HEAD
[0.1.7]: https://github.com/wujunyi0907/Oxizip/releases/tag/v0.1.7
[0.1.6]: https://github.com/wujunyi0907/Oxizip/releases/tag/v0.1.6
[0.1.5]: https://github.com/wujunyi0907/Oxizip/releases/tag/v0.1.5
[0.1.4]: https://github.com/wujunyi0907/Oxizip/releases/tag/v0.1.4
[0.1.3]: https://github.com/wujunyi0907/Oxizip/releases/tag/v0.1.3
[0.1.2]: https://github.com/wujunyi0907/Oxizip/releases/tag/v0.1.2
[0.1.1]: https://github.com/wujunyi0907/Oxizip/releases/tag/v0.1.1
[0.1.0]: https://github.com/wujunyi0907/Oxizip/releases/tag/v0.1.0
