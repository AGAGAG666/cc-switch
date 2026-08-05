//! `tauri-plugin-updater` 的 Android 替身。
//!
//! 原版用点只有 `commands/settings.rs` 两处，调用链固定为：
//! ```ignore
//! let updater = app.updater_builder().build().map_err(..)?;
//! let Some(update) = updater.check().await.map_err(..)? else { return Ok(false) };
//! let bytes = update.download(|chunk_len, content_len| {..}, || {}).await?;
//! update.install(bytes)?;                      // update.version 也被读取
//! ```
//!
//! Android 上 cc-switch 是 ZeroTermux 内的 sidecar，自更新由宿主 APK 负责，
//! sidecar 不能自己替换二进制。因此 `check()` 恒返回 `Ok(None)`：
//!   - `install_update_and_restart` 走 `else` 分支返回 `Ok(false)`
//!   - `check_app_update_available` 返回 `Ok(None)`
//!
//! 两者都是原版已定义的合法「无更新」语义，前端无需改动，
//! 且 `download`/`install` 永远不会被调用到。

use std::marker::PhantomData;

use tauri_shim::{AppHandle, Runtime};

/// 复刻 `tauri_plugin_updater::Error`（调用点只用 `Display`）。
#[derive(Debug)]
pub struct Error(String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// 一个可用更新。Android 上永不构造，仅保持类型与方法签名以便编译。
pub struct Update {
    pub version: String,
    pub current_version: String,
    pub body: Option<String>,
}

impl Update {
    /// 签名对齐原版：进度回调 `FnMut(usize, Option<u64>)` + 完成回调 `FnOnce()`。
    pub async fn download<C, D>(&self, _on_chunk: C, _on_done: D) -> Result<Vec<u8>>
    where
        C: FnMut(usize, Option<u64>) + Send + 'static,
        D: FnOnce() + Send + 'static,
    {
        Err(Error(
            "Android 版本的更新由 ZeroTermux 负责，sidecar 不下载更新".into(),
        ))
    }

    pub fn install(&self, _bytes: Vec<u8>) -> Result<()> {
        Err(Error(
            "Android 版本的更新由 ZeroTermux 负责，sidecar 不安装更新".into(),
        ))
    }
}

/// 复刻 `Updater`。
pub struct Updater<R: Runtime> {
    _marker: PhantomData<R>,
}

impl<R: Runtime> Updater<R> {
    /// Android 恒无更新——见模块文档。
    pub async fn check(&self) -> Result<Option<Update>> {
        log::debug!("sidecar 不自更新，report 无可用更新");
        Ok(None)
    }
}

/// 复刻 `UpdaterBuilder`。
pub struct UpdaterBuilder<R: Runtime> {
    _marker: PhantomData<R>,
}

impl<R: Runtime> UpdaterBuilder<R> {
    /// 原版还有 `timeout`/`proxy`/`header` 等链式方法，cc-switch 未使用。
    pub fn build(self) -> Result<Updater<R>> {
        Ok(Updater {
            _marker: PhantomData,
        })
    }
}

/// 复刻 `UpdaterExt`。
pub trait UpdaterExt<R: Runtime> {
    fn updater_builder(&self) -> UpdaterBuilder<R>;
}

impl<R: Runtime> UpdaterExt<R> for AppHandle<R> {
    fn updater_builder(&self) -> UpdaterBuilder<R> {
        UpdaterBuilder {
            _marker: PhantomData,
        }
    }
}
