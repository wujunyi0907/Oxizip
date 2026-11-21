fn main() {
    #[cfg(windows)]
    {
        let def = std::path::Path::new("oxizip-shellext.def");
        if def.exists() {
            let abs = std::fs::canonicalize(def).expect("canonicalize def");
            println!("cargo:rustc-cdylib-link-arg=/DEF:{}", abs.display());
        }
    }
}