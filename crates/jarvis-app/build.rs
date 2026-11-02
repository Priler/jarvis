fn main() {
    if cfg!(target_os = "windows") {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let lib_path = std::path::Path::new(&manifest_dir)
            .join("..")
            .join("..")
            .join("lib")
            .join("windows")
            .join("amd64");

        println!("cargo:rustc-link-search=native={}", lib_path.display());
    }
}
