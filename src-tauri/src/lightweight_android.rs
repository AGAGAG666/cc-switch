//! `lightweight.rs` 的 Android 替身。
//!
//! 桌面版「轻量模式」= 销毁主窗口、只留托盘常驻。Android sidecar 是无窗口的
//! 后台进程，宿主（ZeroTermux）自己管理 WebView 生命周期，这个概念不存在。
//!
//! `commands/lightweight.rs` 的三个命令仍在命令表里（前端可能调用），
//! 因此保持签名一致：查询恒为 false，进入返回错误让前端显式失败，
//! 退出视为已满足直接成功。

/// Android 无窗口可销毁，明确报不支持，避免前端以为已生效。
pub fn enter_lightweight_mode(_app: &tauri::AppHandle) -> Result<(), String> {
    Err("Android 版本不支持轻量模式（sidecar 本身即无窗口后台进程）".to_string())
}

/// 语义是「确保窗口可见」。Android 上窗口由宿主管理，视为已满足。
pub fn exit_lightweight_mode(_app: &tauri::AppHandle) -> Result<(), String> {
    Ok(())
}

/// 恒为 false：Android 上不存在轻量模式状态。
pub fn is_lightweight_mode() -> bool {
    false
}
