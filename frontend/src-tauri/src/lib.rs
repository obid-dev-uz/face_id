mod camera;
mod commands;
mod db;
mod engine;
mod enroll;
mod error;
mod face;
mod recognize;
mod state;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_khoface_camera::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let conn = db::open(&dir.join("khoface.db"))?;
            match commands::logs::apply_retention(&conn) {
                Ok(n) if n > 0 => log::info!("jurnal: {n} ta eski yozuv o'chirildi"),
                Ok(_) => {}
                Err(e) => log::warn!("jurnalni tozalab bo'lmadi: {e}"),
            }
            app.manage(state::AppState::new(conn, dir));
            engine::spawn_loader(app.handle().clone());
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
            commands::persons::wipe_all_data,
            commands::system::set_secure_screen,
            commands::persons::list_person_samples,
            commands::persons::delete_person_sample,
            commands::logs::list_logs,
            commands::logs::log_snapshot,
            commands::logs::purge_logs,
            commands::settings::get_settings,
            commands::settings::set_setting,
            commands::backup::backup_database,
            commands::backup::restore_database,
            commands::camera::camera_permission,
            commands::camera::request_camera_permission,
            commands::camera::open_app_settings,
            commands::camera::camera_availability,
            commands::camera::start_stream,
            commands::camera::stop_stream,
            commands::camera::camera_ack,
            commands::enroll::enroll_start,
            commands::enroll::enroll_cancel,
            commands::enroll::enroll_state,
            commands::enroll::enroll_remove_sample,
            commands::enroll::enroll_add_image,
            commands::enroll::enroll_commit,
            commands::engine::engine_status,
            commands::engine::analyze_image,
            commands::engine::benchmark_engine,
        ])
        .run(tauri::generate_context!())
        .expect("KHOFace ishga tushmadi");
}
