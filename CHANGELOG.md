# Changelog

本文档记录 Oxizip 面向用户的版本变更，格式参考 [Keep a Changelog](https://keepachangelog.com/)。

版本号遵循 [Semantic Versioning](https://semver.org/)。发版 Git 标签格式为 **`v` + 版本号**（例：`v1.0.1`），`Cargo.toml` 中只写 `1.0.1`（无 `v`）。打标签前必须更新本文件，并将 `[Unreleased]` 归入新版本（见 [AGENTS.md](AGENTS.md)）。

## [Unreleased]

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

[Unreleased]: https://github.com/wujunyi0907/Oxizip/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/wujunyi0907/Oxizip/releases/tag/v0.1.3
[0.1.2]: https://github.com/wujunyi0907/Oxizip/releases/tag/v0.1.2
[0.1.1]: https://github.com/wujunyi0907/Oxizip/releases/tag/v0.1.1
[0.1.0]: https://github.com/wujunyi0907/Oxizip/releases/tag/v0.1.0
