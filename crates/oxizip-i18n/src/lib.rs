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
        (Lang::ZhCn, "error.entry_not_found") => "未找到条目",
        (Lang::En, "error.entry_not_found") => "Entry not found",
        (Lang::ZhCn, "error.other") => "发生错误",
        (Lang::En, "error.other") => "An error occurred",

        (Lang::ZhCn, "gui.open_archive") => "打开",
        (Lang::En, "gui.open_archive") => "Open",
        (Lang::ZhCn, "gui.extract") => "解压到…",
        (Lang::En, "gui.extract") => "Extract…",
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
        | CoreError::Other(detail) => format!("{base}: {detail}"),
        CoreError::EntryNotFound(name) => format!("{base}: {name}"),
        _ => base,
    }
}

pub fn code_message(lang: Lang, code: ErrorCode) -> String {
    t_owned(lang, &format!("error.{}", code.as_str()))
}
