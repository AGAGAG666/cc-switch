//! 命令分发。`#[tauri::command]` 展开出的 `__rpc_*` 函数由这里统一调度。
//!
//! 原版路径：前端 `invoke(name, args)` -> Tauri IPC -> 命令函数。
//! 这里：      前端 `invoke(name, args)` -> `POST /rpc/{name}` -> 同一个命令函数。

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;

use crate::app::{AppHandle, HostBridge, Manager, Wry};
use crate::state::State;
use crate::window::Window;

/// 宏生成的处理函数统一签名。
pub type HandlerFn =
    fn(RpcContext, Value) -> Pin<Box<dyn Future<Output = Result<Value, String>> + Send + 'static>>;

/// 单次调用的上下文，供宏注入 State / AppHandle / Window。
#[derive(Clone)]
pub struct RpcContext {
    app: AppHandle<Wry>,
}

impl RpcContext {
    pub fn new(app: AppHandle<Wry>) -> Self {
        Self { app }
    }

    pub fn app_handle(&self) -> &AppHandle<Wry> {
        &self.app
    }

    pub fn host(&self) -> &Arc<dyn HostBridge> {
        self.app.host()
    }

    /// 供宏注入 `State<'_, T>`。未注册状态时返回错误而非 panic——
    /// sidecar 不该因为一个命令写错就整体崩掉。
    pub fn state<T: Send + Sync + 'static>(&self) -> Result<State<'static, T>, String> {
        Manager::<Wry>::try_state::<T>(&self.app).ok_or_else(|| {
            format!("状态 {} 未注册", std::any::type_name::<T>())
        })
    }

    /// Android 无窗口；原版仅 `set_window_theme` 一处用到 Window。
    pub fn window(&self) -> Window {
        Window::new(self.app.clone())
    }
}

/// 从 JSON body 取出并反序列化一个命令参数。
///
/// 缺键时传 `Null`，让 `Option<T>` 自然得到 `None`——与原版 IPC 行为一致。
/// 同时兼容 snake_case 兜底，避免前端个别调用点用了下划线写法。
pub fn take_arg<T: serde::de::DeserializeOwned>(args: &Value, key: &str) -> Result<T, String> {
    let raw = args
        .get(key)
        .or_else(|| {
            let snake = to_snake(key);
            if snake == key {
                None
            } else {
                args.get(&snake)
            }
        })
        .cloned()
        .unwrap_or(Value::Null);
    serde_json::from_value(raw).map_err(|e| format!("参数 {key} 解析失败: {e}"))
}

fn to_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for ch in s.chars() {
        if ch.is_ascii_uppercase() {
            out.push('_');
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// 命令错误统一转字符串（原版错误类型有 `String` 和 `AppError` 两种，都是 Display）。
pub fn stringify_err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// 命令表。由 `generate_handler!` 构造。
pub struct Handlers {
    map: HashMap<&'static str, HandlerFn>,
}

impl Handlers {
    pub fn from_pairs(pairs: Vec<(&'static str, HandlerFn)>) -> Self {
        let mut map = HashMap::with_capacity(pairs.len());
        for (name, f) in pairs {
            if map.insert(name, f).is_some() {
                log::warn!("命令 {name} 重复注册，后者覆盖前者");
            }
        }
        Self { map }
    }

    /// 合并另一张表（sidecar 用来追加 Android 专属命令，而不去改共享命令表）。
    pub fn extend(&mut self, other: Handlers) {
        for (name, f) in other.map {
            if self.map.insert(name, f).is_some() {
                log::warn!("命令 {name} 重复注册，后者覆盖前者");
            }
        }
    }

    pub fn get(&self, name: &str) -> Option<HandlerFn> {
        self.map.get(name).copied()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// 已注册命令名，便于 sidecar 暴露 `/rpc` 自检清单。
    pub fn names(&self) -> Vec<&'static str> {
        let mut v: Vec<_> = self.map.keys().copied().collect();
        v.sort_unstable();
        v
    }

    /// 分发一次调用。
    pub async fn dispatch(
        &self,
        name: &str,
        ctx: RpcContext,
        args: Value,
    ) -> Result<Value, String> {
        let handler = self
            .get(name)
            .ok_or_else(|| format!("未知命令: {name}"))?;
        handler(ctx, args).await
    }
}
