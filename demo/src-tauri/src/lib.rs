mod commands;
mod models;
mod plc;
mod state;

pub fn run() {
    let mut context = tauri::generate_context!();
    context.set_default_window_icon(Some(tauri::include_image!("icons/icon.png")));

    tauri::Builder::default()
        .manage(state::PlcState::default())
        .invoke_handler(tauri::generate_handler![
            commands::connection::connect,
            commands::connection::disconnect,
            commands::connection::status,
            commands::data::read,
            commands::data::write,
        ])
        .run(context)
        .expect("启动桌面示例失败");
}
