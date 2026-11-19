//! Minimal bilingual message table (zh-CN / en).

use oxizip_core::{CoreError, ErrorCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    ZhCn,
    En,
}

fn lookup(lang: Lang, key: &str) -> Option<&'static str> {
    Some(match (lang, key) {
        (_, "app.name") => "Oxizip",
        (Lang::ZhCn, "cli.done") => "完成",
        (Lang::En, "cli.done") => "Done",
        (Lang::ZhCn, "cli.canceled") => "已取消",
        (Lang::En, "cli.canceled") => "Canceled",
        (Lang::ZhCn, "error.io") => "读写错误",
        (Lang::En, "error.io") => "I/O error",
        (Lang::ZhCn, "error.canceled") => "操作已取消",
        (Lang::En, "error.canceled") => "Operation canceled",
        (Lang::ZhCn, "error.path_unsafe") => "不安全的归档路径",
        (Lang::En, "error.path_unsafe") => "Unsafe archive path",
        (Lang::ZhCn, "error.unsupported_format") => "不支持的格式",
        (Lang::En, "error.unsupported_format") => "Unsupported format",
        (Lang::ZhCn, "error.not_an_archive") => "不是有效的压缩包",
        (Lang::En, "error.not_an_archive") => "Not a valid archive",
        (Lang::ZhCn, "error.password_required") => "需要密码",
        (Lang::En, "error.password_required") => "Password required",
        (Lang::ZhCn, "error.invalid_password") => "密码错误",
        (Lang::En, "error.invalid_password") => "Invalid password",
        (Lang::ZhCn, "error.unrar_missing") => {
            "未找到 UnRAR：Windows 请放置官方 UnRAR.exe；其他系统请安装 unrar"
        }
        (Lang::En, "error.unrar_missing") => {
            "UnRAR not found: place official UnRAR.exe on Windows, or install system unrar elsewhere"
        }
        (Lang::ZhCn, "error.unrar_failed") => "UnRAR 执行失败",
        (Lang::En, "error.unrar_failed") => "UnRAR failed",
        (Lang::ZhCn, "error.wimlib_missing") => {
            "未找到 wimlib-imagex：请安装 wimlib（Windows 可放置 wimlib-imagex.exe）"
        }
        (Lang::En, "error.wimlib_missing") => {
            "wimlib-imagex not found: install wimlib, or place wimlib-imagex.exe on Windows"
        }
        (Lang::ZhCn, "error.wimlib_failed") => "wimlib 执行失败",
        (Lang::En, "error.wimlib_failed") => "wimlib failed",
        (Lang::ZhCn, "error.entry_not_found") => "未找到条目",
        (Lang::En, "error.entry_not_found") => "Entry not found",
        (Lang::ZhCn, "error.other") => "发生错误",
        (Lang::En, "error.other") => "An error occurred",

        (Lang::ZhCn, "gui.open_archive") => "打开",
        (Lang::En, "gui.open_archive") => "Open",
        (Lang::ZhCn, "gui.extract") => "解压到…",
        (Lang::En, "gui.extract") => "Extract…",
        (Lang::ZhCn, "gui.extract_here") => "解压到此处",
        (Lang::En, "gui.extract_here") => "Extract Here",
        (Lang::ZhCn, "gui.extract_smart") => "智能解压",
        (Lang::En, "gui.extract_smart") => "Extract Smart",
        (Lang::ZhCn, "gui.test") => "测试",
        (Lang::En, "gui.test") => "Test",
        (Lang::ZhCn, "gui.add") => "添加…",
        (Lang::En, "gui.add") => "Add…",
        (Lang::ZhCn, "gui.delete_entry") => "删除",
        (Lang::En, "gui.delete_entry") => "Delete",
        (Lang::ZhCn, "gui.create") => "压缩…",
        (Lang::En, "gui.create") => "Compress…",
        (Lang::ZhCn, "gui.cancel") => "取消",
        (Lang::En, "gui.cancel") => "Cancel",
        (Lang::ZhCn, "gui.open_entry") => "打开",
        (Lang::En, "gui.open_entry") => "Open",
        (Lang::ZhCn, "gui.format") => "格式",
        (Lang::En, "gui.format") => "Format",
        (Lang::ZhCn, "gui.method") => "方法",
        (Lang::En, "gui.method") => "Method",
        (Lang::ZhCn, "gui.password") => "密码",
        (Lang::En, "gui.password") => "Password",
        (Lang::ZhCn, "gui.lang") => "语言",
        (Lang::En, "gui.lang") => "Language",
        (Lang::ZhCn, "gui.placeholder") => "Oxizip — 打开压缩包，或选择格式后压缩文件",
        (Lang::En, "gui.placeholder") => "Oxizip — open an archive, or compress files",
        (Lang::ZhCn, "gui.col.name") => "名称",
        (Lang::En, "gui.col.name") => "Name",
        (Lang::ZhCn, "gui.col.size") => "大小",
        (Lang::En, "gui.col.size") => "Size",
        (Lang::ZhCn, "gui.col.method") => "方法",
        (Lang::En, "gui.col.method") => "Method",
        (Lang::ZhCn, "gui.col.action") => "操作",
        (Lang::En, "gui.col.action") => "Action",
        (Lang::ZhCn, "gui.busy.list") => "正在列出…",
        (Lang::En, "gui.busy.list") => "Listing…",
        (Lang::ZhCn, "gui.busy.extract") => "正在解压…",
        (Lang::En, "gui.busy.extract") => "Extracting…",
        (Lang::ZhCn, "gui.busy.create") => "正在压缩…",
        (Lang::En, "gui.busy.create") => "Compressing…",
        (Lang::ZhCn, "gui.busy.open") => "正在打开条目…",
        (Lang::En, "gui.busy.open") => "Opening entry…",
        (Lang::ZhCn, "gui.busy.test") => "正在测试…",
        (Lang::En, "gui.busy.test") => "Testing…",
        (Lang::ZhCn, "gui.busy.add") => "正在添加…",
        (Lang::En, "gui.busy.add") => "Adding…",
        (Lang::ZhCn, "gui.busy.delete") => "正在删除…",
        (Lang::En, "gui.busy.delete") => "Deleting…",
        (Lang::ZhCn, "gui.status.tested") => "测试通过",
        (Lang::En, "gui.status.tested") => "Test OK",
        (Lang::ZhCn, "gui.status.edited") => "已更新归档",
        (Lang::En, "gui.status.edited") => "Archive updated",
        (Lang::ZhCn, "gui.status.ready") => "就绪",
        (Lang::En, "gui.status.ready") => "Ready",
        (Lang::ZhCn, "gui.status.opened") => "已打开",
        (Lang::En, "gui.status.opened") => "Opened",
        (Lang::ZhCn, "gui.status.extracted") => "解压完成",
        (Lang::En, "gui.status.extracted") => "Extracted",
        (Lang::ZhCn, "gui.status.created") => "压缩完成",
        (Lang::En, "gui.status.created") => "Created",
        (Lang::ZhCn, "gui.status.canceled") => "已取消",
        (Lang::En, "gui.status.canceled") => "Canceled",
        (Lang::ZhCn, "gui.err.need_archive") => "请先打开压缩包",
        (Lang::En, "gui.err.need_archive") => "Open an archive first",
        (Lang::ZhCn, "gui.err.need_inputs") => "未选择要压缩的文件",
        (Lang::En, "gui.err.need_inputs") => "No files selected to compress",
        (Lang::ZhCn, "gui.err.dir_open") => "目录请使用解压",
        (Lang::En, "gui.err.dir_open") => "Use Extract for directories",
        (Lang::ZhCn, "gui.err.system_open") => "无法用系统程序打开",
        (Lang::En, "gui.err.system_open") => "Could not open with system app",
        (Lang::ZhCn, "gui.method.auto") => "默认",
        (Lang::En, "gui.method.auto") => "Default",
        (Lang::ZhCn, "gui.entries") => "项",
        (Lang::En, "gui.entries") => "items",
        (Lang::ZhCn, "gui.licenses") => "开源组件",
        (Lang::En, "gui.licenses") => "Licenses",
        (Lang::ZhCn, "gui.licenses.title") => "依赖组件与协议",
        (Lang::En, "gui.licenses.title") => "Dependencies & licenses",
        (Lang::ZhCn, "gui.licenses.back") => "返回",
        (Lang::En, "gui.licenses.back") => "Back",
        (Lang::ZhCn, "gui.licenses.filter") => "分类",
        (Lang::En, "gui.licenses.filter") => "Filter",
        (Lang::ZhCn, "gui.licenses.filter_all") => "全部",
        (Lang::En, "gui.licenses.filter_all") => "All",
        (Lang::ZhCn, "gui.licenses.intro") => {
            "以下为 Oxizip 主要依赖与运行时工具的许可证摘要；完整说明见仓库 README.md。Oxizip 本体为 AGPL-3.0-only。"
        }
        (Lang::En, "gui.licenses.intro") => {
            "Summary of major dependencies and runtime tools. Full notes are in README.md. Oxizip itself is AGPL-3.0-only."
        }
        (Lang::ZhCn, "gui.licenses.col.kind") => "类别",
        (Lang::En, "gui.licenses.col.kind") => "Kind",
        (Lang::ZhCn, "gui.licenses.col.name") => "组件",
        (Lang::En, "gui.licenses.col.name") => "Component",
        (Lang::ZhCn, "gui.licenses.col.license") => "协议",
        (Lang::En, "gui.licenses.col.license") => "License",
        (Lang::ZhCn, "gui.licenses.col.purpose") => "用途",
        (Lang::En, "gui.licenses.col.purpose") => "Purpose",
        (Lang::ZhCn, "gui.licenses.col.note") => "说明",
        (Lang::En, "gui.licenses.col.note") => "Notes",
        (Lang::ZhCn, "gui.licenses.footer") => {
            "传递依赖未全部列出；捆绑 UnRAR / wimlib 时须另遵守其许可。"
        }
        (Lang::En, "gui.licenses.footer") => {
            "Transitive crates are not fully listed; bundling UnRAR/wimlib requires their terms."
        },

        (Lang::ZhCn, "shell.open_with_oxizip") => "用 Oxizip 打开",
        (Lang::En, "shell.open_with_oxizip") => "Open with Oxizip",
        (Lang::ZhCn, "gui.setup.title") => "将文件扩展名关联至 Oxizip",
        (Lang::En, "gui.setup.title") => "Associate file extensions with Oxizip",
        (Lang::ZhCn, "gui.setup.intro") => {
            "选择要由 Oxizip 打开的格式；同时会安装资源管理器右键菜单。可稍后再次打开本页调整。"
        }
        (Lang::En, "gui.setup.intro") => {
            "Choose formats Oxizip should open. Explorer context menus are installed too. You can revisit this later."
        }
        (Lang::ZhCn, "gui.setup.list") => "扩展名关联",
        (Lang::En, "gui.setup.list") => "Extension association",
        (Lang::ZhCn, "gui.setup.group.archive") => "压缩包文件",
        (Lang::En, "gui.setup.group.archive") => "Archive files",
        (Lang::ZhCn, "gui.setup.group.unix") => "UNIX",
        (Lang::En, "gui.setup.group.unix") => "UNIX",
        (Lang::ZhCn, "gui.setup.group.other") => "其它",
        (Lang::En, "gui.setup.group.other") => "Others",
        (Lang::ZhCn, "gui.setup.group.disk") => "磁盘映像",
        (Lang::En, "gui.setup.group.disk") => "Disk images",
        (Lang::ZhCn, "gui.setup.select_all") => "全选",
        (Lang::En, "gui.setup.select_all") => "Select All",
        (Lang::ZhCn, "gui.setup.clear_all") => "全不选",
        (Lang::En, "gui.setup.clear_all") => "Clear All",
        (Lang::ZhCn, "gui.setup.apply") => "立即应用",
        (Lang::En, "gui.setup.apply") => "Apply Now",
        (Lang::ZhCn, "gui.setup.skip") => "暂时跳过",
        (Lang::En, "gui.setup.skip") => "Skip for now",
        (Lang::ZhCn, "gui.setup.none") => "请至少选择一种格式，或点击暂时跳过",
        (Lang::En, "gui.setup.none") => "Select at least one format, or skip for now",
        (Lang::ZhCn, "gui.setup.ok") => "已完成文件关联",
        (Lang::En, "gui.setup.ok") => "File associations installed",
        (Lang::ZhCn, "gui.setup.failed") => "文件关联失败",
        (Lang::En, "gui.setup.failed") => "Failed to install associations",
        (Lang::ZhCn, "gui.setup.menu") => "文件关联…",
        (Lang::En, "gui.setup.menu") => "Associations…",
        (Lang::ZhCn, "shell.extract_here") => "Oxizip 解压到此处",
        (Lang::En, "shell.extract_here") => "Oxizip Extract Here",
        (Lang::ZhCn, "shell.extract_smart") => "Oxizip 智能解压",
        (Lang::En, "shell.extract_smart") => "Oxizip Extract Here (Smart)",
        (Lang::ZhCn, "shell.extract_to_folder") => "Oxizip 解压到归档名文件夹",
        (Lang::En, "shell.extract_to_folder") => "Oxizip Extract to Archive Folder",
        (Lang::ZhCn, "shell.test") => "Oxizip 测试压缩包",
        (Lang::En, "shell.test") => "Oxizip Test Archive",
        (Lang::ZhCn, "shell.checksum") => "Oxizip 校验和信息",
        (Lang::En, "shell.checksum") => "Oxizip Checksum Information",
        (Lang::ZhCn, "shell.repair") => "Oxizip 修复压缩包",
        (Lang::En, "shell.repair") => "Oxizip Repair Archive",
        (Lang::ZhCn, "shell.compress_zip") => "Oxizip 压缩为 ZIP",
        (Lang::En, "shell.compress_zip") => "Oxizip Compress to ZIP",
        (Lang::ZhCn, "shell.compress_7z") => "Oxizip 压缩为 7z",
        (Lang::En, "shell.compress_7z") => "Oxizip Compress to 7z",
        (Lang::ZhCn, "shell.desktop_extract") => "Oxizip 解压",
        (Lang::En, "shell.desktop_extract") => "Oxizip Extract",
        (Lang::ZhCn, "shell.desktop_compress") => "Oxizip 压缩",
        (Lang::En, "shell.desktop_compress") => "Oxizip Compress",
        (Lang::ZhCn, "shell.install_ok") => "已安装当前用户的文件管理器集成",
        (Lang::En, "shell.install_ok") => "Installed per-user file manager integration",
        (Lang::ZhCn, "shell.uninstall_ok") => "已移除 Oxizip 文件管理器集成",
        (Lang::En, "shell.uninstall_ok") => "Removed Oxizip file manager integration",
        (Lang::ZhCn, "shell.status_installed") => "文件管理器集成：已安装",
        (Lang::En, "shell.status_installed") => "Shell integration: installed",
        (Lang::ZhCn, "shell.status_missing") => "文件管理器集成：未安装",
        (Lang::En, "shell.status_missing") => "Shell integration: not installed",
        _ => return None,
    })
}

/// Translate a known key. Unknown keys fall back to the key string (owned).
pub fn t_owned(lang: Lang, key: &str) -> String {
    lookup(lang, key)
        .map(str::to_string)
        .unwrap_or_else(|| key.to_string())
}

/// Translate a known static key.
pub fn t(lang: Lang, key: &'static str) -> &'static str {
    lookup(lang, key).unwrap_or(key)
}

pub fn error_message(lang: Lang, err: &CoreError) -> String {
    let base = t_owned(lang, &err.i18n_key());
    match err {
        CoreError::Io(e) => format!("{base}: {e}"),
        CoreError::PathUnsafe(detail)
        | CoreError::UnrarFailed(detail)
        | CoreError::WimlibFailed(detail)
        | CoreError::Other(detail) => format!("{base}: {detail}"),
        CoreError::EntryNotFound(name) => format!("{base}: {name}"),
        _ => base,
    }
}

pub fn code_message(lang: Lang, code: ErrorCode) -> String {
    t_owned(lang, &format!("error.{}", code.as_str()))
}
