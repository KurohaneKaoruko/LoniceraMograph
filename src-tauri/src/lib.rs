mod agent_tools;
mod ai;
mod commands;
mod demo;
mod events;
mod scene;
mod script;
mod workspace;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::get_default_scene,
            commands::get_frame,
            commands::chat,
            commands::save_export,
            commands::list_files,
            commands::read_file,
            commands::write_file,
            commands::delete_file,
            commands::rename_file,
            commands::open_scene,
            commands::run_script,
            commands::workspace_root,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
