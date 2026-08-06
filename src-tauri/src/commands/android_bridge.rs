//! Android/WebView 桥专用命令。
//!
//! 桌面前端有两处能力来自 Tauri 内建的 IPC（`@tauri-apps/api/app` 的 `getVersion`、
//! `@tauri-apps/api/path` 的 `homeDir`），它们不在 `generate_handler!` 的命令表里，
//! 而是 Tauri 运行时自带。sidecar 只暴露命令表，所以这两条能力必须补成真命令，
//! 否则前端替身只能去 invoke 一个 404 的名字。
//!
//! 只在 Android 目标下编译并注册，桌面命令表逐字不变。

/// 应用版本号。与桌面 `getVersion()` 同源：都取自 Cargo.toml 的 `package.version`
/// （tauri.conf.json 的 version 字段也是从这里同步的）。
#[tauri::command]
pub async fn get_app_version() -> Result<String, String> {
    Ok(env!("CARGO_PKG_VERSION").to_string())
}

/// 用户 HOME 目录。复用 `config::get_home_dir()`，与 Rust 侧所有路径推导同一来源，
/// 避免前端自己拼 `/data/data/com.termux/files/home` 之类的硬编码。
#[tauri::command]
pub async fn get_home_dir() -> Result<String, String> {
    crate::config::get_home_dir()
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "HOME 路径包含非 UTF-8 字符".to_string())
}
