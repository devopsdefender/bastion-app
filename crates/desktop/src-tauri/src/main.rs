// Prevents additional console window on Windows in release, DO NOT
// REMOVE — this is the Tauri v2-recommended entry.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    bastion_desktop_lib::run();
}
