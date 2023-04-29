fn main() {
    // link to Vosk lib
    println!("cargo:rustc-link-lib=libvosk.dll");

    // Tauri build
    tauri_build::build()
}
