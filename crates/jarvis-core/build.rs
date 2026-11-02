fn main() {
    if let Ok(path) = std::env::var("VOSK_LIB_DIR") {
        println!("cargo:rustc-link-search=native={}", path);
    }

    if cfg!(target_os = "linux") {
        for path in ["/usr/local/lib", "/usr/lib/x86_64-linux-gnu", "/usr/lib"] {
            println!("cargo:rustc-link-search=native={}", path);
        }
    }
}
