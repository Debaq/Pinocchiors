// Sin comandos para el WebView: se usa solo desde Rust (apps/desktop/src/android_files.rs)
const COMMANDS: &[&str] = &[];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).android_path("android").build();
}
