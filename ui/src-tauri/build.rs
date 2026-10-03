fn main() {
    // tauri.windows.conf.json lists resources/agg-svc.exe. `tauri dev` still
    // validates that path even though only the NSIS bundle needs the real binary.
    let dest = std::path::Path::new("resources/agg-svc.exe");
    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if !dest.exists() {
        let _ = std::fs::write(dest, []);
    }
    tauri_build::build()
}
