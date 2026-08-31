// lib.rs — Tauri library crate entry point

mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // This is a library crate; the actual binary entry point is main.rs
}
