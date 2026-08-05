//! `tauri` 的 Android sidecar 替身。
//!
//! ## 为什么存在
//!
//! cc-switch 有 293 处 `#[tauri::command]`、64 处 `tauri::State`、54 处
//! `tauri::AppHandle`。逐个改写既易错又会让上游 rebase 变成灾难。
//!
//! 这个 crate 在 `Cargo.toml` 里用 `package = "tauri-shim"` 重命名为 `tauri`：
//!
//! ```toml
//! [target.'cfg(target_os = "android")'.dependencies]
//! tauri = { package = "tauri-shim", path = "android/tauri-shim" }
//! ```
//!
//! 于是 `commands/`（33 文件）、`proxy/`、`services/` **一行不改**即可编译，
//! 上游同步只需处理真实冲突。
//!
//! ## 覆盖范围
//!
//! 已核实的 Tauri 符号使用面（全 `src/` 统计）全部覆盖：
//! `command`、`generate_handler!`、`State`、`AppHandle`、`Manager`、`Emitter`、
//! `Runtime`/`Wry`、`Window`、`Theme`、`async_runtime::{spawn, spawn_blocking,
//! block_on}`、`process::restart`、`RESTART_EXIT_CODE`，以及
//! opener / dialog / store 三个插件的 Ext trait。
//!
//! 桌面专有的重量级符号（`Builder`、`RunEvent`、`tray`、`menu`、
//! `WebviewWindowBuilder`、`image::Image`、updater）**不**在此实现——它们只出现在
//! `lib.rs` / `tray.rs` / `lightweight.rs`，Android 上整体 `cfg` 掉。

pub mod app;
pub mod plugins;
pub mod rpc;
pub mod runtime;
pub mod state;
pub mod window;

use std::path::PathBuf;
use std::sync::OnceLock;

// ── 与 `tauri::` 顶层同名的再导出 ──────────────────────────────

pub use app::{
    AppHandle, Emitter, EventSink, HostBridge, Manager, NoopEventSink, NoopHostBridge, Runtime, Wry,
};
pub use state::State;
pub use window::{Theme, Window};

/// 复刻 `tauri::async_runtime`。
pub mod async_runtime {
    pub use crate::runtime::{block_on, spawn, spawn_blocking, JoinHandle};
}

/// 复刻 `tauri::process`。
pub mod process {
    /// 原版签名是 `restart(&Env) -> !`。Android 上进程重启由 Java 层负责，
    /// 这里保持 `-> !` 语义（调用点 `lib.rs:2213` 依赖它永不返回）。
    pub fn restart(_env: &crate::Env) -> ! {
        log::warn!("sidecar 请求重启，交由宿主处理");
        std::process::exit(crate::RESTART_EXIT_CODE);
    }
}

/// 复刻 `tauri::Error` 中被 cc-switch 实际观测到的部分。
///
/// 调用点普遍写 `if let Err(e) = app.emit(..)`，只用到 `Display`。
#[derive(Debug)]
pub enum Error {
    /// 事件载荷序列化失败。
    Serialize(serde_json::Error),
    /// 其它宿主侧错误。
    Host(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Serialize(e) => write!(f, "序列化失败: {e}"),
            Error::Host(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for Error {}

/// 复刻 `tauri::Result`。
pub type Result<T> = std::result::Result<T, Error>;

/// 原版 `app.env()` 的返回值。Android 上无内容，仅占位以保持调用形状。
pub struct Env;

/// 与原版一致：`app.restart()` 使用的退出码。
pub const RESTART_EXIT_CODE: i32 = 51;

/// `#[tauri::command]` —— 由 `tauri-shim-macros` 提供，语义见该 crate。
pub use tauri_shim_macros::command;

/// `tauri::generate_handler!` —— 展开为 `Vec<(&str, HandlerFn)>`，
/// 供 `rpc::Handlers::from_pairs` 构建 `POST /rpc/{cmd}` 路由表。
pub use tauri_shim_macros::generate_handler;

// ── sidecar 运行期配置 ────────────────────────────────────────

static STORE_BASE_DIR: OnceLock<PathBuf> = OnceLock::new();

/// 设置 `store_builder()` 的落盘根目录（sidecar 启动时调用一次）。
///
/// 桌面版 store 插件写在 app config dir；Android 上由宿主指定，
/// 通常是 `~/.cc-switch`。
pub fn set_store_base_dir(dir: impl Into<PathBuf>) {
    if STORE_BASE_DIR.set(dir.into()).is_err() {
        log::warn!("store 根目录已设置，忽略重复设置");
    }
}

/// 读取 store 根目录；未设置时回落到 `~/.cc-switch`。
pub fn store_base_dir() -> PathBuf {
    if let Some(dir) = STORE_BASE_DIR.get() {
        return dir.clone();
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cc-switch")
}
