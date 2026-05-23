use std::path::Path;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let lib_path = Path::new(&manifest_dir).join("..\\..\\lib\\windows\\amd64");

    // compile-time link search path
    println!("cargo:rustc-link-search=native={}", lib_path.display());

    // Copy DLLs into the binary output directory so the OS can find them at runtime.
    // Windows searches the EXE's own directory first; placing DLLs there is the
    // simplest cross-machine solution (no PATH manipulation needed).
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let target_dir = Path::new(&out_dir)
        .parent().unwrap()  // .../build/jarvis-app-<hash>
        .parent().unwrap()  // .../build
        .parent().unwrap(); // target/debug  (or target/release)

    if let Ok(entries) = std::fs::read_dir(&lib_path) {
        for entry in entries.flatten() {
            let src = entry.path();
            if src.extension().map_or(false, |e| e.eq_ignore_ascii_case("dll")) {
                let dst = target_dir.join(src.file_name().unwrap());
                if let Err(e) = std::fs::copy(&src, &dst) {
                    println!("cargo:warning=Failed to copy {} to {}: {}", src.display(), dst.display(), e);
                } else {
                    println!("cargo:warning=Copied {} to {}", src.file_name().unwrap().to_string_lossy(), dst.display());
                }
                println!("cargo:rerun-if-changed={}", src.display());
            }
        }
    }
}
