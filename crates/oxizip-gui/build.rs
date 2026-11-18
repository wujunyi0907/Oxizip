fn main() {
    println!("cargo:rerun-if-changed=assets/icons/oxizip.ico");
    embed_windows_icon();
}

#[cfg(windows)]
fn embed_windows_icon() {
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/icons/oxizip.ico");
    res.set("ProductName", "Oxizip");
    res.set("FileDescription", "Oxizip Archive Manager");
    res.set("LegalCopyright", "Oxizip contributors");
    if let Err(e) = res.compile() {
        println!("cargo:warning=failed to embed Windows icon: {e}");
    }
}

#[cfg(not(windows))]
fn embed_windows_icon() {}
