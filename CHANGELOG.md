# Changelog

本文档记录 Oxizip 面向用户的版本变更，格式参考 [Keep a Changelog](https://keepachangelog.com/)。

版本号遵循 [Semantic Versioning](https://semver.org/)。发版 Git 标签格式为 **`v` + 版本号**（例：`v1.0.1`），`Cargo.toml` 中只写 `1.0.1`（无 `v`）。打标签前必须更新本文件，并将 `[Unreleased]` 归入新版本（见 [AGENTS.md](AGENTS.md)）。

## [Unreleased]

### Changed

- 分发产物文件名统一为 `Oxizip` 前缀；RPM 仍按 NEVRA 保留 `Release`（如 `Oxizip-<ver>-1.<arch>.rpm`）
- GitHub Release 仅在 Setup / deb / rpm / AppImage / tar.gz 全部齐全时创建
- Package 工作流仅在打 `v*` 标签或手动触发时运行（避免与 `main` 推送重复打包）
- 完善「变更范围」「提交纪律」、Conventional Commit（前缀 + 中文）与 PR 说明规范（见 [AGENTS.md](AGENTS.md)）

### Added

- 英文说明 [README.en.md](README.en.md)
- [CHANGELOG.md](CHANGELOG.md)；发版标签格式明确为 `vX.Y.Z`

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

[Unreleased]: https://github.com/wujunyi0907/Oxizip/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/wujunyi0907/Oxizip/releases/tag/v0.1.2
[0.1.1]: https://github.com/wujunyi0907/Oxizip/releases/tag/v0.1.1
[0.1.0]: https://github.com/wujunyi0907/Oxizip/releases/tag/v0.1.0
