//! `tray.rs` 的 Android 替身。
//!
//! Android sidecar 没有系统托盘。原版 `tray.rs`（1537 行）整体只在桌面编译，
//! 但托盘刷新的调用点散落在保留文件里：
//!   - `commands/profile.rs`      -> `refresh_tray_menu`
//!   - `commands/failover.rs`     -> `refresh_tray_menu`
//!   - `proxy/failover_switch.rs` -> `refresh_tray_menu`
//!   - `commands/provider.rs`     -> `schedule_tray_refresh`
//!   - `commands/subscription.rs` -> `schedule_tray_refresh`
//!
//! 这些位置的语义是「顺带同步托盘展示」，主流程（写库 + emit 事件）已在调用前
//! 完成，因此 Android 上安全降级为 no-op。用 `#[path]` 在 `lib.rs` 里按 target
//! 切换实现，调用点一行不改。

/// 与桌面版同名，供潜在的日志/诊断引用。
pub const TRAY_ID: &str = "cc-switch";

/// 桌面：重建并替换托盘菜单。Android：无托盘，直接返回。
pub fn refresh_tray_menu(_app: &tauri::AppHandle) {}

/// 桌面：50ms 合窗后异步刷新托盘用量标题。Android：无托盘，直接返回。
pub fn schedule_tray_refresh(_app: &tauri::AppHandle) {}
