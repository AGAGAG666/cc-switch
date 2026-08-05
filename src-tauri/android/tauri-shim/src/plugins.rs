//! Tauri 插件 shim：opener / dialog / store。
//!
//! 这三个插件在保留文件里有真实调用点，所以必须提供同名 trait 与同形状 API：
//!   - opener：`app.opener().open_path(..)` / `.open_url(..)`  -> 交给宿主桥（Java Intent）
//!   - dialog：`app.dialog().file()...blocking_pick_*()`        -> 交给宿主桥
//!   - store： `app.store_builder(..).build()` KV + save        -> 真实实现（JSON 文件）
//!
//! updater 未 shim：Android 由 ZeroTermux 自己管更新，相关命令在源码侧 cfg 掉。

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use serde_json::{Map, Value};

use crate::app::{AppHandle, Runtime};

// ─── FilePath ────────────────────────────────────────────────

/// 复刻 `tauri_plugin_dialog::FilePath` 的用到的那部分。
#[derive(Clone, Debug)]
pub struct FilePath(PathBuf);

impl FilePath {
    pub fn new(p: impl Into<PathBuf>) -> Self {
        Self(p.into())
    }

    /// 原版用于去掉 UNC 前缀等；这里路径本就干净，返回自身。
    pub fn simplified(self) -> Self {
        self
    }

    pub fn into_path(self) -> Result<PathBuf, String> {
        Ok(self.0)
    }
}

impl std::fmt::Display for FilePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.to_string_lossy())
    }
}

// ─── opener ──────────────────────────────────────────────────

pub struct Opener<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> Opener<R> {
    /// `with` 参数在原版用于指定打开方式；Android 交由系统选择，忽略。
    pub fn open_path(
        &self,
        path: impl AsRef<str>,
        _with: Option<impl AsRef<str>>,
    ) -> Result<(), String> {
        self.app.host().open_path(path.as_ref())
    }

    pub fn open_url(
        &self,
        url: impl AsRef<str>,
        _with: Option<impl AsRef<str>>,
    ) -> Result<(), String> {
        self.app.host().open_url(url.as_ref())
    }
}

pub trait OpenerExt<R: Runtime> {
    fn opener(&self) -> Opener<R>;
}

impl<R: Runtime> OpenerExt<R> for AppHandle<R> {
    fn opener(&self) -> Opener<R> {
        Opener { app: self.clone() }
    }
}

// ─── dialog ──────────────────────────────────────────────────

pub struct Dialog<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> Dialog<R> {
    pub fn file(&self) -> FileDialogBuilder<R> {
        FileDialogBuilder {
            app: self.app.clone(),
            extensions: Vec::new(),
            file_name: None,
            directory: None,
        }
    }
}

/// 复刻 `FileDialogBuilder` 的链式 API（builder 方法取所有权返回 Self）。
pub struct FileDialogBuilder<R: Runtime> {
    app: AppHandle<R>,
    extensions: Vec<String>,
    file_name: Option<String>,
    directory: Option<PathBuf>,
}

impl<R: Runtime> FileDialogBuilder<R> {
    pub fn add_filter(mut self, _name: impl Into<String>, extensions: &[&str]) -> Self {
        self.extensions
            .extend(extensions.iter().map(|e| (*e).to_string()));
        self
    }

    pub fn set_file_name(mut self, name: impl Into<String>) -> Self {
        self.file_name = Some(name.into());
        self
    }

    pub fn set_directory(mut self, dir: impl AsRef<Path>) -> Self {
        self.directory = Some(dir.as_ref().to_path_buf());
        self
    }

    pub fn set_title(self, _title: impl Into<String>) -> Self {
        self
    }

    fn ext_refs(&self) -> Vec<&str> {
        self.extensions.iter().map(String::as_str).collect()
    }

    pub fn blocking_pick_folder(self) -> Option<FilePath> {
        self.app.host().pick_folder().map(FilePath::new)
    }

    pub fn blocking_pick_file(self) -> Option<FilePath> {
        let exts = self.ext_refs();
        self.app.host().pick_file(&exts).map(FilePath::new)
    }

    pub fn blocking_save_file(self) -> Option<FilePath> {
        let exts = self.ext_refs();
        let name = self.file_name.clone().unwrap_or_default();
        self.app.host().save_file(&name, &exts).map(FilePath::new)
    }
}

pub trait DialogExt<R: Runtime> {
    fn dialog(&self) -> Dialog<R>;
}

impl<R: Runtime> DialogExt<R> for AppHandle<R> {
    fn dialog(&self) -> Dialog<R> {
        Dialog { app: self.clone() }
    }
}

// ─── store ───────────────────────────────────────────────────

/// 复刻 `tauri_plugin_store::Store` 的 get/set/delete/save 四件套。
/// 真实读写 JSON 文件，语义与原版一致（原版也是 JSON 落盘）。
#[derive(Clone)]
pub struct Store {
    path: PathBuf,
    data: Arc<RwLock<Map<String, Value>>>,
}

impl Store {
    fn load(path: PathBuf) -> Self {
        let data = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<Map<String, Value>>(&s).ok())
            .unwrap_or_default();
        Self {
            path,
            data: Arc::new(RwLock::new(data)),
        }
    }

    pub fn get(&self, key: impl AsRef<str>) -> Option<Value> {
        self.data
            .read()
            .ok()?
            .get(key.as_ref())
            .cloned()
    }

    pub fn set(&self, key: impl Into<String>, value: Value) {
        if let Ok(mut guard) = self.data.write() {
            guard.insert(key.into(), value);
        }
    }

    pub fn delete(&self, key: impl AsRef<str>) -> bool {
        self.data
            .write()
            .map(|mut g| g.remove(key.as_ref()).is_some())
            .unwrap_or(false)
    }

    pub fn save(&self) -> Result<(), String> {
        let guard = self
            .data
            .read()
            .map_err(|_| "Store 读写锁被污染".to_string())?;
        let body = serde_json::to_string_pretty(&*guard)
            .map_err(|e| format!("序列化 Store 失败: {e}"))?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("创建 Store 目录失败: {e}"))?;
        }
        std::fs::write(&self.path, body).map_err(|e| format!("写入 Store 失败: {e}"))
    }
}

pub struct StoreBuilder {
    path: PathBuf,
}

impl StoreBuilder {
    pub fn build(self) -> Result<Store, String> {
        Ok(Store::load(self.path))
    }
}

pub trait StoreExt<R: Runtime> {
    fn store_builder(&self, path: impl AsRef<Path>) -> StoreBuilder;
}

impl<R: Runtime> StoreExt<R> for AppHandle<R> {
    fn store_builder(&self, path: impl AsRef<Path>) -> StoreBuilder {
        // 原版把 store 文件放在 app config dir 下；这里沿用 cc-switch 的配置根，
        // 保持与桌面版同样的「配置目录内一个 JSON」语义。
        let base = crate::store_base_dir();
        StoreBuilder {
            path: base.join(path.as_ref()),
        }
    }
}
