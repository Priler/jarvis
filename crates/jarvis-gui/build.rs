fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let lib_path = std::path::Path::new(&manifest_dir)
        .join("..\\..\\lib\\windows\\amd64");

    println!("cargo:rustc-link-search=native={}", lib_path.display());

    tauri_build::build()
}
