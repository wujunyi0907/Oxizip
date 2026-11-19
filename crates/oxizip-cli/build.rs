fn main() {
    let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("oxizip-gui")
        .join("assets")
        .join("icons")
        .join("oxizip.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    embed_windows_icon(&icon);
    compile_user_choice_latest();
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

#[cfg(windows)]
fn compile_user_choice_latest() {
    let native = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("native")
        .join("user_choice_latest");
    for name in [
        "HashCodec.cpp",
        "HashTables.cpp",
        "oxizip_ucl_api.cpp",
        "HashCommon.h",
        "oxizip_ucl_api.h",
        "UserChoiceLatestHashTables.inc",
    ] {
        println!("cargo:rerun-if-changed={}", native.join(name).display());
    }

    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .include(&native)
        .file(native.join("HashCodec.cpp"))
        .file(native.join("HashTables.cpp"))
        .file(native.join("oxizip_ucl_api.cpp"))
        .compile("oxizip_user_choice_latest");

    println!("cargo:rustc-link-lib=advapi32");
    println!("cargo:rustc-link-lib=crypt32");
}

#[cfg(not(windows))]
fn compile_user_choice_latest() {}
