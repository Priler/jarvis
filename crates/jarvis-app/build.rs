fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let lib_path = std::path::Path::new(&manifest_dir)
        .join("..")
        .join("..")
        .join("lib")
        .join("windows")
        .join("amd64");

    println!("cargo:rustc-link-search=native={}", lib_path.display());

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let profile = std::env::var("PROFILE").unwrap();
    let target_dir = out_dir
        .ancestors()
        .find(|path| path.file_name().is_some_and(|name| name == std::ffi::OsStr::new(&profile)))
        .expect("Cargo target profile directory not found");

    for entry in std::fs::read_dir(&lib_path).unwrap() {
        let entry = entry.unwrap();
        let source = entry.path();
        if source.extension().is_some_and(|extension| extension == "dll") {
            let destination = target_dir.join(entry.file_name());
            std::fs::copy(&source, destination).unwrap();
            println!("cargo:rerun-if-changed={}", source.display());
        }
    }
}
