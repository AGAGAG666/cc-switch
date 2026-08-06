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
/// `keys` 由宏按优先级给出：第一个是 tauri 语义下的 camelCase 键名（前端实际
/// 发送的形态），其后是 Rust 字面参数名作兜底。两者相同时只查一次。
///
/// 全部候选键都缺失时传 `Null`，让 `Option<T>` 自然得到 `None`——与原版 IPC
/// 行为一致。注意"键存在但值为 null"与"键不存在"在这里等价，这也和原版一致
/// （前端把 `undefined` 序列化成缺键，把 `null` 序列化成 null）。
pub fn take_arg<T: serde::de::DeserializeOwned>(args: &Value, keys: &[&str]) -> Result<T, String> {
    let raw = keys
        .iter()
        .find_map(|k| args.get(*k))
        .filter(|v| !v.is_null())
        .cloned()
        .unwrap_or(Value::Null);
    let key = keys.first().copied().unwrap_or("");
    serde_json::from_value(raw).map_err(|e| format!("参数 {key} 解析失败: {e}"))
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

#[cfg(test)]
mod tests {
    use super::take_arg;
    use serde_json::json;

    #[test]
    fn 首选键优先于兜底键() {
        let args = json!({ "appType": "codex", "app_type": "claude" });
        let v: String = take_arg(&args, &["appType", "app_type"]).unwrap();
        assert_eq!(v, "codex");
    }

    #[test]
    fn 首选键缺失时回落到兜底键() {
        let args = json!({ "app_type": "claude" });
        let v: String = take_arg(&args, &["appType", "app_type"]).unwrap();
        assert_eq!(v, "claude");
    }

    #[test]
    fn 首选键为_null_时跳到兜底键() {
        // 关键：前端漏发字段时序列化成 null，不能因此挡住兜底键。
        let args = json!({ "appType": null, "app_type": "claude" });
        let v: String = take_arg(&args, &["appType", "app_type"]).unwrap();
        assert_eq!(v, "claude");
    }

    #[test]
    fn 全部候选键缺失时_option_得到_none() {
        let args = json!({});
        let v: Option<String> = take_arg(&args, &["appType", "app_type"]).unwrap();
        assert_eq!(v, None);
        let v: Option<String> = take_arg(&json!({ "appType": null }), &["appType"]).unwrap();
        assert_eq!(v, None);
    }

    #[test]
    fn 必填参数缺失时错误信息用首选键() {
        let err = take_arg::<String>(&json!({}), &["appType", "app_type"]).unwrap_err();
        assert!(err.starts_with("参数 appType 解析失败"), "{err}");
    }

    #[test]
    fn 类型不符时报错而非_panic() {
        let err = take_arg::<String>(&json!({ "appType": 7 }), &["appType"]).unwrap_err();
        assert!(err.contains("参数 appType 解析失败"), "{err}");
    }

    #[test]
    fn 重复候选键不影响结果() {
        // 已是 camelCase 的参数名转换后与字面名相同，宏会传两个一样的键。
        let v: String = take_arg(&json!({ "providerId": "p" }), &["providerId", "providerId"])
            .unwrap();
        assert_eq!(v, "p");
    }

    #[test]
    fn 结构体入参正常反序列化() {
        #[derive(serde::Deserialize, PartialEq, Debug)]
        struct P {
            id: String,
        }
        let v: P = take_arg(&json!({ "payload": { "id": "x" } }), &["payload"]).unwrap();
        assert_eq!(v, P { id: "x".into() });
    }
}
