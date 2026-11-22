#[tauri::command]
pub fn tray_panel_state() -> Result<(), String> { Err("Desktop tray panel is unavailable on Android".into()) }
#[tauri::command]
pub fn tray_panel_placement() -> Result<(), String> { Err("Desktop tray panel is unavailable on Android".into()) }
#[tauri::command]
pub fn tray_panel_layout() -> Result<(), String> { Err("Desktop tray panel is unavailable on Android".into()) }
#[tauri::command]
pub fn tray_panel_hide() -> Result<(), String> { Err("Desktop tray panel is unavailable on Android".into()) }
#[tauri::command]
pub fn tray_panel_action() -> Result<(), String> { Err("Desktop tray panel is unavailable on Android".into()) }
