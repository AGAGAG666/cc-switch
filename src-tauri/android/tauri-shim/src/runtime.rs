//! `tauri::async_runtime` 复刻。
//!
//! 原版是 Tauri 托管的全局 tokio runtime。这里自建一个全局 runtime，
//! 保证 `spawn` / `spawn_blocking` 在**非 async 上下文**里也能用
//! （原版 `usage_events.rs`、自动同步 worker 就是这么调的）。

use std::future::Future;
use std::sync::OnceLock;

use tokio::runtime::{Builder, Runtime};

pub use tokio::task::JoinHandle;

fn global() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        Builder::new_multi_thread()
            .enable_all()
            .thread_name("ccs-sidecar")
            .build()
            .expect("创建全局 tokio runtime 失败")
    })
}

/// 取全局 runtime 句柄，供需要显式 `Handle` 的场景使用。
pub fn handle() -> tokio::runtime::Handle {
    match tokio::runtime::Handle::try_current() {
        Ok(h) => h,
        Err(_) => global().handle().clone(),
    }
}

/// 复刻 `tauri::async_runtime::spawn`。可在同步上下文调用。
pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    handle().spawn(future)
}

/// 复刻 `tauri::async_runtime::spawn_blocking`。
pub fn spawn_blocking<F, T>(func: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    handle().spawn_blocking(func)
}

/// 复刻 `tauri::async_runtime::block_on`（原版仅测试代码使用）。
pub fn block_on<F: Future>(future: F) -> F::Output {
    global().block_on(future)
}
