//! Embedded third-party component catalog for the licenses UI.

use oxizip_i18n::Lang;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentKind {
    SelfProject,
    Embedded,
    ExternalCli,
}

impl ComponentKind {
    pub fn label(self, lang: Lang) -> &'static str {
        let zh = lang == Lang::ZhCn;
        match self {
            Self::SelfProject => {
                if zh {
                    "本项目"
                } else {
                    "This project"
                }
            }
            Self::Embedded => {
                if zh {
                    "嵌入 / 链接"
                } else {
                    "Embedded / linked"
                }
            }
            Self::ExternalCli => {
                if zh {
                    "外部 CLI"
                } else {
                    "External CLI"
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ComponentInfo {
    pub kind: ComponentKind,
    pub name: &'static str,
    pub license: &'static str,
    pub purpose_zh: &'static str,
    pub purpose_en: &'static str,
    pub note_zh: &'static str,
    pub note_en: &'static str,
}

impl ComponentInfo {
    pub fn purpose(self, lang: Lang) -> &'static str {
        if lang == Lang::ZhCn {
            self.purpose_zh
        } else {
            self.purpose_en
        }
    }

    pub fn note(self, lang: Lang) -> &'static str {
        if lang == Lang::ZhCn {
            self.note_zh
        } else {
            self.note_en
        }
    }
}

/// Curated catalog aligned with README license section (direct / notable deps).
pub fn catalog() -> &'static [ComponentInfo] {
    &[
        ComponentInfo {
            kind: ComponentKind::SelfProject,
            name: "Oxizip",
            license: "AGPL-3.0-only",
            purpose_zh: "本软件",
            purpose_en: "This software",
            note_zh: "见仓库根目录 LICENSE",
            note_en: "See root LICENSE",
        },
        ComponentInfo {
            kind: ComponentKind::Embedded,
            name: "ZPAQ (lib/zpaq)",
            license: "Unlicense",
            purpose_zh: ".zpaq 读写（嵌入编译）",
            purpose_en: ".zpaq read/write (embedded)",
            note_zh: "见 lib/zpaq/COPYING",
            note_en: "See lib/zpaq/COPYING",
        },
        ComponentInfo {
            kind: ComponentKind::Embedded,
            name: "libdivsufsort-lite",
            license: "MIT",
            purpose_zh: "嵌入于 libzpaq.cpp",
            purpose_en: "Embedded in libzpaq.cpp",
            note_zh: "版权 Yuta Mori",
            note_en: "Copyright Yuta Mori",
        },
        ComponentInfo {
            kind: ComponentKind::Embedded,
            name: "cab",
            license: "MIT",
            purpose_zh: "Microsoft Cabinet 解压",
            purpose_en: "Microsoft Cabinet extract",
            note_zh: "crates.io",
            note_en: "crates.io",
        },
        ComponentInfo {
            kind: ComponentKind::Embedded,
            name: "lzxd",
            license: "MIT OR Apache-2.0",
            purpose_zh: "CAB 所用 LZX",
            purpose_en: "LZX used by CAB",
            note_zh: "cab 依赖",
            note_en: "cab dependency",
        },
        ComponentInfo {
            kind: ComponentKind::Embedded,
            name: "unalz",
            license: "MIT",
            purpose_zh: "ALZ 解压",
            purpose_en: "ALZ extract",
            note_zh: "crates.io",
            note_en: "crates.io",
        },
        ComponentInfo {
            kind: ComponentKind::Embedded,
            name: "unegg",
            license: "MIT",
            purpose_zh: "EGG 解压",
            purpose_en: "EGG extract",
            note_zh: "crates.io",
            note_en: "crates.io",
        },
        ComponentInfo {
            kind: ComponentKind::Embedded,
            name: "libazo",
            license: "MIT",
            purpose_zh: "EGG AZO 解压",
            purpose_en: "EGG AZO decompress",
            note_zh: "unegg 依赖",
            note_en: "unegg dependency",
        },
        ComponentInfo {
            kind: ComponentKind::Embedded,
            name: "sevenz-turbo",
            license: "Apache-2.0",
            purpose_zh: "7z 读写",
            purpose_en: "7z read/write",
            note_zh: "crates.io",
            note_en: "crates.io",
        },
        ComponentInfo {
            kind: ComponentKind::Embedded,
            name: "zip",
            license: "MIT",
            purpose_zh: "ZIP / ZIPX 读写",
            purpose_en: "ZIP / ZIPX read/write",
            note_zh: "crates.io",
            note_en: "crates.io",
        },
        ComponentInfo {
            kind: ComponentKind::Embedded,
            name: "tar / flate2 / bzip2",
            license: "MIT OR Apache-2.0",
            purpose_zh: "tar 族与编解码",
            purpose_en: "tar family & codecs",
            note_zh: "crates.io",
            note_en: "crates.io",
        },
        ComponentInfo {
            kind: ComponentKind::Embedded,
            name: "lzma-rs",
            license: "MIT",
            purpose_zh: "LZMA / XZ",
            purpose_en: "LZMA / XZ",
            note_zh: "crates.io",
            note_en: "crates.io",
        },
        ComponentInfo {
            kind: ComponentKind::Embedded,
            name: "zstd",
            license: "BSD-3-Clause",
            purpose_zh: "Zstandard",
            purpose_en: "Zstandard",
            note_zh: "crates.io / 上游 Meta zstd",
            note_en: "crates.io / upstream Meta zstd",
        },
        ComponentInfo {
            kind: ComponentKind::Embedded,
            name: "crc32fast / sha2",
            license: "MIT OR Apache-2.0",
            purpose_zh: "校验和",
            purpose_en: "Checksums",
            note_zh: "crates.io",
            note_en: "crates.io",
        },
        ComponentInfo {
            kind: ComponentKind::Embedded,
            name: "iced / rfd / open",
            license: "MIT",
            purpose_zh: "GUI / 对话框 / 系统打开",
            purpose_en: "GUI / dialogs / system open",
            note_zh: "crates.io",
            note_en: "crates.io",
        },
        ComponentInfo {
            kind: ComponentKind::Embedded,
            name: "clap / thiserror / tempfile",
            license: "MIT OR Apache-2.0",
            purpose_zh: "CLI / 错误 / 临时文件",
            purpose_en: "CLI / errors / temp files",
            note_zh: "crates.io",
            note_en: "crates.io",
        },
        ComponentInfo {
            kind: ComponentKind::Embedded,
            name: "tracing / tracing-subscriber",
            license: "MIT",
            purpose_zh: "日志",
            purpose_en: "Logging",
            note_zh: "crates.io",
            note_en: "crates.io",
        },
        ComponentInfo {
            kind: ComponentKind::Embedded,
            name: "walkdir",
            license: "Unlicense/MIT",
            purpose_zh: "目录遍历",
            purpose_en: "Directory walk",
            note_zh: "crates.io",
            note_en: "crates.io",
        },
        ComponentInfo {
            kind: ComponentKind::Embedded,
            name: "winreg",
            license: "MIT",
            purpose_zh: "Windows 壳注册",
            purpose_en: "Windows shell registration",
            note_zh: "仅 Windows",
            note_en: "Windows only",
        },
        ComponentInfo {
            kind: ComponentKind::ExternalCli,
            name: "UnRAR",
            license: "UnRAR freeware",
            purpose_zh: "RAR 仅解压（官方 CLI）",
            purpose_en: "RAR extract-only (official CLI)",
            note_zh: "不链接 DLL；禁止逆向重建 RAR 压缩算法",
            note_en: "No DLL link; reverse-engineering RAR compression forbidden",
        },
        ComponentInfo {
            kind: ComponentKind::ExternalCli,
            name: "wimlib / wimlib-imagex",
            license: "GPLv3+ (libwim: LGPLv2.1+ optional)",
            purpose_zh: "WIM 仅解压（官方 CLI）",
            purpose_en: "WIM extract-only (official CLI)",
            note_zh: "仅 spawn；捆绑分发须履行 GPL/LGPL",
            note_en: "Spawn only; bundling requires GPL/LGPL compliance",
        },
    ]
}
