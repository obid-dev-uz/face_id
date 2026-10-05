mod camera;
mod commands;
mod db;
mod error;
mod face;
mod state;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let conn = db::open(&dir.join("khoface.db"))?;
            app.manage(state::AppState::new(conn, dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::app_info,
            commands::persons::list_persons,
            commands::persons::get_person,
            commands::persons::create_person,
            commands::persons::update_person,
            commands::persons::set_person_active,
            commands::persons::delete_person,
            commands::logs::list_logs,
            commands::logs::purge_logs,
            commands::settings::get_settings,
            commands::settings::set_setting,
            commands::backup::backup_database,
            commands::backup::restore_database,
        ])
        .run(tauri::generate_context!())
        .expect("KHOFace ishga tushmadi");
}
