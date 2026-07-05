// GUI subsystem always (not just release): a standalone launch must never pop
// a console window. When run from a terminal (cargo run / tauri dev), stdout is
// still inherited, so logs remain visible during development.
#![windows_subsystem = "windows"]

fn main() {
    openflow_desktop_lib::run()
}
