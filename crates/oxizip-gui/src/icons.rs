//! Application and format icons from `assets/icons/*.ico`.

use std::collections::HashMap;
use std::io::Cursor;

use iced::widget::image as iced_image;
use iced::window;
use oxizip_engine::FormatId;

macro_rules! ico {
    ($file:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/",
            $file
        ))
    };
}

const OXIZIP_ICO: &[u8] = ico!("oxizip.ico");

/// Window / taskbar icon (embedded).
pub fn window_icon() -> Option<window::Icon> {
    window::icon::from_file_data(OXIZIP_ICO, Some(::image::ImageFormat::Ico)).ok()
}

/// Decoded RGBA handles for list / UI, keyed by lowercase extension / format id.
pub struct IconSet {
    by_key: HashMap<&'static str, iced_image::Handle>,
    fallback: iced_image::Handle,
}

impl IconSet {
    pub fn load() -> Self {
        let pairs: &[(&str, &[u8])] = &[
            ("oxizip", OXIZIP_ICO),
            ("zip", ico!("zip.ico")),
            ("zipx", ico!("zipx.ico")),
            ("7z", ico!("7z.ico")),
            ("rar", ico!("rar.ico")),
            ("tar", ico!("tar.ico")),
            ("tgz", ico!("tgz.ico")),
            ("gz", ico!("gz.ico")),
            ("xz", ico!("xz.ico")),
            ("bz2", ico!("bz2.ico")),
            ("tbz", ico!("tbz.ico")),
            ("tbz2", ico!("tbz2.ico")),
            ("lzh", ico!("lzh.ico")),
            ("lha", ico!("lha.ico")),
            ("iso", ico!("iso.ico")),
            ("zpaq", ico!("zpaq.ico")),
            ("cab", ico!("cab.ico")),
            ("alz", ico!("alz.ico")),
            ("egg", ico!("egg.ico")),
        ];

        let mut by_key = HashMap::new();
        let mut fallback = handle_from_ico(OXIZIP_ICO).unwrap_or_else(empty_handle);
        for (key, bytes) in pairs {
            if let Some(h) = handle_from_ico(bytes) {
                if *key == "oxizip" {
                    fallback = h.clone();
                }
                by_key.insert(*key, h);
            }
        }
        Self { by_key, fallback }
    }

    pub fn for_format(&self, format: FormatId) -> iced_image::Handle {
        let key = match format {
            FormatId::Zip | FormatId::ExeSfx => "zip",
            FormatId::Zipx => "zipx",
            FormatId::SevenZ => "7z",
            FormatId::Rar => "rar",
            FormatId::Tar => "tar",
            FormatId::TarGz => "tgz",
            FormatId::Gz => "gz",
            FormatId::TarXz | FormatId::Xz => "xz",
            FormatId::TarBz2 | FormatId::Bz2 => "bz2",
            FormatId::TarZst | FormatId::Zst => "xz",
            FormatId::Lzh => "lzh",
            FormatId::Iso => "iso",
            FormatId::Zpaq => "zpaq",
            FormatId::Cab => "cab",
            FormatId::Wim => "oxizip",
            FormatId::Alz => "alz",
            FormatId::Egg => "egg",
        };
        self.by_key
            .get(key)
            .cloned()
            .unwrap_or_else(|| self.fallback.clone())
    }

    pub fn for_path_name(&self, name: &str) -> iced_image::Handle {
        let lower = name.to_ascii_lowercase();
        let key = if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
            "tgz"
        } else if lower.ends_with(".tar.bz2") || lower.ends_with(".tbz2") {
            "tbz2"
        } else if lower.ends_with(".tbz") {
            "tbz"
        } else if lower.ends_with(".tar.xz") || lower.ends_with(".txz") {
            "xz"
        } else if lower.ends_with(".tar.zst") {
            "xz"
        } else if let Some(ext) = std::path::Path::new(&lower)
            .extension()
            .and_then(|e| e.to_str())
        {
            match ext {
                "zip" | "jar" => "zip",
                "zipx" => "zipx",
                "7z" => "7z",
                "rar" => "rar",
                "tar" => "tar",
                "gz" => "gz",
                "xz" => "xz",
                "bz2" => "bz2",
                "lzh" => "lzh",
                "lha" => "lha",
                "iso" => "iso",
                "zpaq" => "zpaq",
                "cab" => "cab",
                "alz" => "alz",
                "egg" => "egg",
                "exe" => "zip",
                _ => "oxizip",
            }
        } else {
            "oxizip"
        };
        self.by_key
            .get(key)
            .cloned()
            .unwrap_or_else(|| self.fallback.clone())
    }

    pub fn oxizip(&self) -> iced_image::Handle {
        self.fallback.clone()
    }
}

fn handle_from_ico(data: &[u8]) -> Option<iced_image::Handle> {
    let mut reader = ::image::io::Reader::new(Cursor::new(data));
    reader.set_format(::image::ImageFormat::Ico);
    let rgba = reader.decode().ok()?.to_rgba8();
    let (w, h, pixels) = if rgba.width() > 32 || rgba.height() > 32 {
        let resized = ::image::imageops::resize(
            &rgba,
            16,
            16,
            ::image::imageops::FilterType::Triangle,
        );
        (16, 16, resized.into_raw())
    } else {
        (rgba.width(), rgba.height(), rgba.into_raw())
    };
    Some(iced_image::Handle::from_rgba(w, h, pixels))
}

fn empty_handle() -> iced_image::Handle {
    iced_image::Handle::from_rgba(1, 1, vec![0, 0, 0, 0])
}
