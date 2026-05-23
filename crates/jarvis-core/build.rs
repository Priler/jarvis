fn main() {
    // When the vosk feature is enabled on Windows, point the linker at libvosk.lib
    // and copy libvosk.dll into the output directory so benches and tests can run.
    if std::env::var("CARGO_FEATURE_VOSK").is_ok() {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let lib_path = std::path::Path::new(&manifest_dir)
            .join("..").join("..").join("lib").join("windows").join("amd64");

        if lib_path.exists() {
            println!("cargo:rustc-link-search=native={}", lib_path.display());

            let out_dir = std::env::var("OUT_DIR").unwrap();
            let target_dir = std::path::Path::new(&out_dir)
                .parent().unwrap()  // build/<crate>-<hash>
                .parent().unwrap()  // build/
                .parent().unwrap(); // target/debug or target/release

            if let Ok(entries) = std::fs::read_dir(&lib_path) {
                for entry in entries.flatten() {
                    let src = entry.path();
                    if src.extension().map_or(false, |e| e.eq_ignore_ascii_case("dll")) {
                        let dst = target_dir.join(src.file_name().unwrap());
                        if let Err(e) = std::fs::copy(&src, &dst) {
                            println!("cargo:warning=Failed to copy {}: {}", src.display(), e);
                        }
                        println!("cargo:rerun-if-changed={}", src.display());
                    }
                }
            }
        }
    }
}
