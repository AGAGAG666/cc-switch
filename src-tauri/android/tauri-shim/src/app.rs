//! `AppHandle` 与 `Manager` / `Emitter` trait。
//!
//! 原版 AppHandle 是「窗口 + 状态 + 事件总线 + 插件」的句柄。在 Android sidecar 里
//! 没有窗口，所以它退化为「状态表 + 事件汇 + 宿主回调」三件套：
//!   - 状态表  -> 复刻 `manage` / `state`
//!   - 事件汇  -> `emit` 转 SSE 广播（前端 `listen()` 照旧收到）
//!   - 宿主回调 -> opener/dialog/restart 交给 Java 层
//!
//! 核心逻辑（proxy/services）只把 AppHandle 当不透明句柄传递并偶尔 `.emit()`，
//! 因此这个退化版本足以让那些文件一行不改地编译并正确工作。

use std::sync::Arc;

use crate::state::{State, StateMap};

/// 事件汇。Android 侧由 SSE 广播实现，桌面测试可用 no-op。
pub trait EventSink: Send + Sync {
    fn dispatch(&self, event: &str, payload: serde_json::Value);
}

/// 丢弃所有事件的事件汇（单元测试 / sidecar 启动早期用）。
pub struct NoopEventSink;

impl EventSink for NoopEventSink {
    fn dispatch(&self, _event: &str, _payload: serde_json::Value) {}
}

/// 宿主能力回调，对应桌面版的 opener / dialog / process 插件。
///
/// Android 上这些动作要么交给 Java 层走 Intent，要么无意义直接报错，
/// 由 sidecar 注入具体实现。
pub trait HostBridge: Send + Sync {
    fn open_url(&self, url: &str) -> Result<(), String> {
        Err(format!("当前平台不支持打开外部链接: {url}"))
    }
    fn open_path(&self, path: &str) -> Result<(), String> {
        Err(format!("当前平台不支持打开路径: {path}"))
    }
    fn pick_folder(&self) -> Option<String> {
        None
    }
    fn pick_file(&self, _extensions: &[&str]) -> Option<String> {
        None
    }
    fn save_file(&self, _default_name: &str, _extensions: &[&str]) -> Option<String> {
        None
    }
    fn restart(&self) {
        log::warn!("宿主未实现 restart，忽略");
    }
    fn exit(&self, code: i32) {
        log::warn!("宿主未实现 exit({code})，忽略");
    }
}

/// 默认宿主：全部动作不可用。
pub struct NoopHostBridge;
impl HostBridge for NoopHostBridge {}

/// 供 `#[tauri::command]` 宏识别的运行时占位类型参数。
///
/// 原版 `AppHandle<R: Runtime>` 有 3 处泛型命令（import_export.rs 的文件对话框）。
/// 这里保留同名 trait 和默认参数 `Wry`，让那些签名无需改动。
pub trait Runtime: Send + Sync + 'static {}

/// 原版默认运行时名。Android 上只是个标记类型。
pub struct Wry;
impl Runtime for Wry {}

struct AppInner {
    states: StateMap,
    events: Arc<dyn EventSink>,
    host: Arc<dyn HostBridge>,
}

/// 与 `tauri::AppHandle` 等价的句柄。`Clone` 廉价（Arc 计数）。
pub struct AppHandle<R: Runtime = Wry> {
    inner: Arc<AppInner>,
    _marker: std::marker::PhantomData<R>,
}

impl<R: Runtime> Clone for AppHandle<R> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<R: Runtime> AppHandle<R> {
    pub fn new(events: Arc<dyn EventSink>, host: Arc<dyn HostBridge>) -> Self {
        Self {
            inner: Arc::new(AppInner {
                states: StateMap::new(),
                events,
                host,
            }),
            _marker: std::marker::PhantomData,
        }
    }

    /// 无事件、无宿主能力的句柄，用于测试。
    pub fn noop() -> Self {
        Self::new(Arc::new(NoopEventSink), Arc::new(NoopHostBridge))
    }

    /// 暴露宿主桥，供插件 shim（opener/dialog/process）使用。
    pub fn host(&self) -> &Arc<dyn HostBridge> {
        &self.inner.host
    }
}

/// 复刻 `tauri::Manager`：状态注册与读取。
///
/// 原版还提供窗口访问（`get_webview_window`），但那些调用点全在
/// `lib.rs` / `tray.rs` / `lightweight.rs`——Android 上整体 cfg 掉，故不需要。
pub trait Manager<R: Runtime> {
    fn manage<T: Send + Sync + 'static>(&self, state: T);
    fn state<T: Send + Sync + 'static>(&self) -> State<'static, T>;
    fn try_state<T: Send + Sync + 'static>(&self) -> Option<State<'static, T>>;
    fn app_handle(&self) -> AppHandle<R>;
}

impl<R: Runtime> Manager<R> for AppHandle<R> {
    fn manage<T: Send + Sync + 'static>(&self, state: T) {
        self.inner.states.manage(state);
    }

    fn state<T: Send + Sync + 'static>(&self) -> State<'static, T> {
        self.try_state().unwrap_or_else(|| {
            panic!(
                "状态 {} 未注册；请检查 sidecar 启动时的 manage 调用",
                std::any::type_name::<T>()
            )
        })
    }

    fn try_state<T: Send + Sync + 'static>(&self) -> Option<State<'static, T>> {
        self.inner.states.try_get::<T>().map(State::new)
    }

    fn app_handle(&self) -> AppHandle<R> {
        self.clone()
    }
}

/// 复刻 `tauri::Emitter`：向前端推事件。
///
/// 原版 `emit` 失败返回 `tauri::Error`；调用点普遍写
/// `if let Err(e) = app.emit(...)`，所以必须保留 `Result`。
pub trait Emitter<R: Runtime> {
    fn emit<S: serde::Serialize>(&self, event: &str, payload: S) -> crate::Result<()>;
}

impl<R: Runtime> Emitter<R> for AppHandle<R> {
    fn emit<S: serde::Serialize>(&self, event: &str, payload: S) -> crate::Result<()> {
        let value = serde_json::to_value(payload).map_err(crate::Error::Serialize)?;
        self.inner.events.dispatch(event, value);
        Ok(())
    }
}
