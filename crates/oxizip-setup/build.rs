use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace = manifest.join("../..");
    // Installer PE icon (brand + UAC shield). Installed app keeps oxizip.ico.
    let icon = workspace.join("crates/oxizip-gui/assets/icons/oxizip-setup.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    println!(
        "cargo:rerun-if-changed={}",
        workspace
            .join("crates/oxizip-gui/assets/icons/oxizip.ico")
            .display()
    );

    let payload = workspace.join("target/setup-payload/payload.zip");
    println!("cargo:rerun-if-changed={}", payload.display());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let dest = out.join("payload.zip");
    if payload.is_file() {
        fs::copy(&payload, &dest).expect("copy setup payload.zip");
        println!("cargo:rustc-cfg=oxizip_has_payload");
    } else {
        // Allow `cargo check` without packaging; runtime install will error clearly.
        fs::write(&dest, []).expect("write empty payload placeholder");
    }

    #[cfg(windows)]
    {
        if icon.is_file() {
            let mut res = winresource::WindowsResource::new();
            res.set_icon(icon.to_string_lossy().as_ref());
            res.set("ProductName", "Oxizip Setup");
            res.set("FileDescription", "Oxizip Installer");
            res.set("LegalCopyright", "Oxizip contributors");
            let _ = res.compile();
        }
    }
}
