//! `tauri_plugin_opener` 替身。真实实现见 `tauri::plugins`。
//!
//! 单独成 crate 只为让 `use tauri_plugin_opener::OpenerExt;`（4 处）原样编译。
pub use tauri::plugins::{Opener, OpenerExt};
