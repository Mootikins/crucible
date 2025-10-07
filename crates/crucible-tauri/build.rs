fn main() {
    // Skip icon requirements for now
    std::env::set_var("TAURI_SKIP_ICON", "1");
    tauri_build::build()
}
