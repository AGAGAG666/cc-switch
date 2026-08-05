//! `Window` 与 `Theme` 占位。
//!
//! 原版全仓只有一处命令收 `Window` 参数（`set_window_theme`），
//! 其余窗口操作都在 `lib.rs` / `tray.rs` / `lightweight.rs`——Android 上已 cfg 掉。
//! 因此这里只需让签名编译通过，并把主题设置变成无害的空操作。

use crate::app::{AppHandle, Wry};

/// 复刻 `tauri::Theme`。
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    Dark,
}

/// 与 `tauri::Window` 同名的占位类型。
#[derive(Clone)]
pub struct Window {
    app: AppHandle<Wry>,
}

impl Window {
    pub fn new(app: AppHandle<Wry>) -> Self {
        Self { app }
    }

    pub fn app_handle(&self) -> &AppHandle<Wry> {
        &self.app
    }

    /// Android 外观由原生层接管，这里空实现。
    pub fn set_theme(&self, _theme: Option<Theme>) -> crate::Result<()> {
        Ok(())
    }
}
