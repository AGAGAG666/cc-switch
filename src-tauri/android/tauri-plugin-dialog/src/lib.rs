//! `tauri_plugin_dialog` 替身。真实实现见 `tauri_shim::plugins`。
//!
//! 让 `use tauri_plugin_dialog::DialogExt;`（2 处）原样编译。
pub use tauri_shim::plugins::{Dialog, DialogExt, FileDialogBuilder, FilePath};
