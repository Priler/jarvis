fn main() {
    // link to Vosk lib
    println!("cargo:rustc-link-search=vosk/");

    // println!("cargo:rustc-link-lib=dylib=D:/Rust/vosk/libvosk.dll");

    // Tauri build
    tauri_build::build()
}
