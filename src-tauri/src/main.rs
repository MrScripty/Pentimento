//! Pentimento Tauri desktop app entry point

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    pentimento_tauri_lib::run()
}
