fn main() {
    let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("oxizip-gui")
        .join("assets")
        .join("icons")
        .join("oxizip.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    embed_windows_icon(&icon);
}

#[cfg(windows)]
fn embed_windows_icon(icon: &std::path::Path) {
    if !icon.is_file() {
        println!(
            "cargo:warning=icon missing at {}, skipping PE icon embed",
            icon.display()
        );
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon(icon.to_string_lossy().as_ref());
    res.set("ProductName", "Oxizip");
    res.set("FileDescription", "Oxizip CLI");
    res.set("LegalCopyright", "Oxizip contributors");
    if let Err(e) = res.compile() {
        println!("cargo:warning=failed to embed Windows icon: {e}");
    }
}

#[cfg(not(windows))]
fn embed_windows_icon(_icon: &std::path::Path) {}
