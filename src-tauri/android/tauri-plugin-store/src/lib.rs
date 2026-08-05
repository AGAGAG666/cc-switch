//! `tauri_plugin_store` 替身。真实实现见 `tauri_shim::plugins`。
//!
//! 让 `use tauri_plugin_store::StoreExt;`（app_store.rs）原样编译。
pub use tauri_shim::plugins::{Store, StoreBuilder, StoreExt};
