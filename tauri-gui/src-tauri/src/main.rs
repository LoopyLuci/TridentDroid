// main.rs — Tauri application entry point
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod client;
mod streaming;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("tridentd_gui=info".parse().unwrap()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(commands::AppState::default())
        .manage(streaming::StreamingState::default())
        .invoke_handler(tauri::generate_handler![
            commands::ping_daemon,
            commands::launch_instance,
            commands::list_instances,
            commands::stop_instance,
            commands::get_instance_info,
            commands::fork_instance,
            commands::adb_shell_command,
            commands::check_updates,
            commands::get_settings,
            commands::save_settings,
            streaming::start_adb_shell,
            streaming::send_adb_command,
            streaming::start_display_stream,
            streaming::close_adb_shell,
            streaming::close_display_stream,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
