// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(e) = passm_app::run() {
        eprintln!("passm failed to start: {e}");
        std::process::exit(1);
    }
}