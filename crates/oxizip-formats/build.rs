use std::env;
use std::path::PathBuf;

fn main() {
    let pointer_width = env::var("CARGO_CFG_TARGET_POINTER_WIDTH").unwrap_or_default();
    if pointer_width != "64" {
        println!("cargo:warning=Oxizip only supports 64-bit targets (got {pointer_width})");
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("crates/oxizip-formats -> workspace root");
    let zpaq_dir = workspace_root.join("third_party").join("zpaq");

    println!("cargo:rerun-if-changed={}", zpaq_dir.join("zpaq.cpp").display());
    println!("cargo:rerun-if-changed={}", zpaq_dir.join("libzpaq.cpp").display());
    println!("cargo:rerun-if-changed={}", zpaq_dir.join("libzpaq.h").display());
    let native_zpaq = manifest_dir.join("native/zpaq");
    println!(
        "cargo:rerun-if-changed={}",
        native_zpaq.join("oxizip_zpaq.h").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        native_zpaq.join("oxizip_zpaq_capture.cpp").display()
    );

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++14")
        .define("OXIZIP_EMBEDDED", None)
        .include(&zpaq_dir)
        .include(&native_zpaq)
        .file(zpaq_dir.join("libzpaq.cpp"))
        .file(zpaq_dir.join("zpaq.cpp"))
        .file(native_zpaq.join("oxizip_zpaq_capture.cpp"))
        .warnings(false);

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "linux" || target_os == "macos" {
        build.define("unix", None);
        println!("cargo:rustc-link-lib=pthread");
    }

    if target_os == "windows" {
        println!("cargo:rustc-link-lib=advapi32");
        println!("cargo:rustc-link-lib=shell32");
        println!("cargo:rustc-link-lib=user32");
    }

    build.compile("oxizip_zpaq");
}