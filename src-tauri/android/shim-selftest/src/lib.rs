//! 用**真实 cc-switch 命令形态**验证 `tauri-shim` 的宏展开。
//!
//! 每个命令都对照过 `src-tauri/src/` 里的实际签名，覆盖：
//! plain camelCase 参数、`rename_all = "camelCase"`、`State<'_, T>`（含多状态）、
//! `AppHandle`、`AppHandle<R>` + 泛型、`Window`、非 Result 返回、自定义 Display
//! 错误、`Option<T>` 缺键、`Emitter`、三个插件 Ext trait。

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_store::StoreExt;

// ── 状态类型：对照真实的 5 种 ────────────────────────────────

/// 对照 `store.rs` 的 AppState（无 Tauri 依赖）。
pub struct AppState {
    pub label: String,
}

pub struct CopilotAuthState(pub tokio::sync::RwLock<String>);
pub struct XaiOAuthState(pub tokio::sync::RwLock<String>);

/// 对照 39 处使用 `AppError`（thiserror，有 Display）的命令。
#[derive(Debug)]
pub enum AppError {
    NotFound(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::NotFound(k) => write!(f, "未找到: {k}"),
        }
    }
}

impl std::error::Error for AppError {}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct ProviderPayload {
    pub id: String,
    pub enabled: bool,
}

pub mod commands {
    use super::*;

    /// 形态 1：同步 + plain camelCase 参数 + `Result<_, String>`。
    /// 对照 274 个 plain 命令的主流写法。
    #[tauri::command]
    #[allow(non_snake_case)]
    pub fn get_provider(providerId: String) -> Result<ProviderPayload, String> {
        if providerId.is_empty() {
            return Err("id 为空".into());
        }
        Ok(ProviderPayload {
            id: providerId,
            enabled: true,
        })
    }

    /// 形态 2：async + `State<'_, AppState>`。对照 139 处。
    #[tauri::command]
    pub async fn read_state_label(state: State<'_, AppState>) -> Result<String, String> {
        Ok(state.label.clone())
    }

    /// 形态 3：`rename_all = "camelCase"` + 多个 State。
    /// 对照 `commands/auth.rs:103` 的 `auth_start_login`。
    #[tauri::command(rename_all = "camelCase")]
    pub async fn auth_start_login(
        auth_provider: String,
        github_domain: Option<String>,
        copilot_state: State<'_, CopilotAuthState>,
        xai_state: State<'_, XaiOAuthState>,
    ) -> Result<String, String> {
        let a = copilot_state.0.read().await.clone();
        let b = xai_state.0.read().await.clone();
        Ok(format!(
            "{auth_provider}|{}|{a}|{b}",
            github_domain.unwrap_or_else(|| "none".into())
        ))
    }

    /// 形态 4：`AppHandle` + `Emitter`。对照 `provider-switched` 等事件发射点。
    #[tauri::command]
    pub async fn switch_and_emit(app: AppHandle, target: String) -> Result<(), String> {
        app.emit("provider-switched", &target)
            .map_err(|e| e.to_string())
    }

    /// 形态 5：泛型 `<R: tauri::Runtime>` + `AppHandle<R>` + 带属性的参数。
    /// 对照 `commands/import_export.rs:82` 的 `save_file_dialog`。
    #[tauri::command]
    pub async fn save_file_dialog<R: tauri::Runtime>(
        app: tauri::AppHandle<R>,
        #[allow(non_snake_case)] defaultName: String,
    ) -> Result<Option<String>, String> {
        let dialog = app.dialog();
        let result = dialog
            .file()
            .add_filter("SQL", &["sql"])
            .set_file_name(&defaultName)
            .blocking_save_file();
        Ok(result.map(|p| p.to_string()))
    }

    /// 形态 6：`Window` 参数。对照 `commands/misc.rs:3752` 的 `set_window_theme`。
    #[tauri::command]
    pub async fn set_window_theme(window: tauri::Window, theme: String) -> Result<(), String> {
        let t = match theme.as_str() {
            "dark" => Some(tauri::Theme::Dark),
            "light" => Some(tauri::Theme::Light),
            _ => None,
        };
        window.set_theme(t).map_err(|e| e.to_string())
    }

    /// 形态 7：非 Result 返回（`-> Vec<_>`，对照 2 处）。
    #[tauri::command]
    pub fn list_app_types() -> Vec<String> {
        vec!["claude".into(), "codex".into(), "opencode".into()]
    }

    /// 形态 8：非 Result 返回（`-> bool`，对照 1 处）。
    #[tauri::command]
    pub fn is_ready() -> bool {
        true
    }

    /// 形态 9：自定义 Display 错误（对照 39 处 `AppError`）。
    #[tauri::command]
    pub async fn find_thing(key: String) -> Result<String, AppError> {
        if key == "ok" {
            Ok("found".into())
        } else {
            Err(AppError::NotFound(key))
        }
    }

    /// 形态 10：opener 插件（对照 4 处）。
    #[tauri::command]
    pub async fn open_external(app: AppHandle, url: String) -> Result<(), String> {
        app.opener().open_url(&url, None::<String>)
    }

    /// 形态 11：store 插件（对照 `app_store.rs` 1 处）。
    #[tauri::command]
    pub async fn store_roundtrip(app: AppHandle, key: String) -> Result<String, String> {
        let store = app.store_builder("selftest.json").build()?;
        store.set(key.clone(), serde_json::json!("v1"));
        let got = store
            .get(&key)
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        Ok(got)
    }

    /// 形态 12：无参数 + 无返回值（`-> ()`）。
    #[tauri::command]
    pub fn ping() {}

    /// 形态 13：复杂结构体入参（serde 反序列化）。
    #[tauri::command]
    #[allow(non_snake_case)]
    pub async fn upsert_provider(
        state: State<'_, AppState>,
        payload: ProviderPayload,
        dryRun: Option<bool>,
    ) -> Result<String, String> {
        Ok(format!(
            "{}:{}:{}:{}",
            state.label,
            payload.id,
            payload.enabled,
            dryRun.unwrap_or(false)
        ))
    }
}

/// 构造带全部状态的 AppHandle + 命令表，模拟 sidecar 启动。
pub fn build() -> (AppHandle, tauri::rpc::Handlers) {
    let app = AppHandle::noop();
    app.manage(AppState {
        label: "sidecar".into(),
    });
    app.manage(CopilotAuthState(tokio::sync::RwLock::new("cop".into())));
    app.manage(XaiOAuthState(tokio::sync::RwLock::new("xai".into())));

    // 对照 lib.rs:1318 的 generate_handler! 块（含 `::` 路径）。
    let handlers = tauri::generate_handler![
        commands::get_provider,
        commands::read_state_label,
        commands::auth_start_login,
        commands::switch_and_emit,
        commands::save_file_dialog,
        commands::set_window_theme,
        commands::list_app_types,
        commands::is_ready,
        commands::find_thing,
        commands::open_external,
        commands::store_roundtrip,
        commands::ping,
        commands::upsert_provider,
    ];
    (app, handlers)
}

/// 记录 emit 的事件汇，验证 `Emitter` 真的把事件送出去了。
pub struct RecordingSink(pub std::sync::Mutex<Vec<(String, serde_json::Value)>>);

impl tauri::EventSink for RecordingSink {
    fn dispatch(&self, event: &str, payload: serde_json::Value) {
        self.0
            .lock()
            .unwrap()
            .push((event.to_string(), payload));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tauri::rpc::RpcContext;

    async fn call(name: &str, args: serde_json::Value) -> Result<serde_json::Value, String> {
        let (app, handlers) = build();
        handlers.dispatch(name, RpcContext::new(app), args).await
    }

    #[tokio::test]
    async fn 全部命令都注册了() {
        let (_, handlers) = build();
        assert_eq!(handlers.len(), 13, "names={:?}", handlers.names());
        // 命令名必须是原函数名，不能带 __rpc_ 前缀。
        assert!(handlers.names().contains(&"get_provider"));
        assert!(handlers.names().iter().all(|n| !n.starts_with("__rpc_")));
    }

    #[tokio::test]
    async fn plain_camelcase_参数按字面名取值() {
        let out = call("get_provider", json!({ "providerId": "p1" }))
            .await
            .expect("应成功");
        assert_eq!(out, json!({ "id": "p1", "enabled": true }));
    }

    #[tokio::test]
    async fn 命令的_err_被转成字符串() {
        let err = call("get_provider", json!({ "providerId": "" }))
            .await
            .unwrap_err();
        assert_eq!(err, "id 为空");
    }

    #[tokio::test]
    async fn state_注入可用() {
        let out = call("read_state_label", json!({})).await.unwrap();
        assert_eq!(out, json!("sidecar"));
    }

    #[tokio::test]
    async fn rename_all_把_snake_case_参数转成_camelcase() {
        // 前端传 camelCase，命令签名是 snake_case。
        let out = call(
            "auth_start_login",
            json!({ "authProvider": "github-copilot", "githubDomain": "example.com" }),
        )
        .await
        .unwrap();
        assert_eq!(out, json!("github-copilot|example.com|cop|xai"));
    }

    #[tokio::test]
    async fn option_参数缺键得到_none() {
        let out = call("auth_start_login", json!({ "authProvider": "x" }))
            .await
            .unwrap();
        assert_eq!(out, json!("x|none|cop|xai"));
    }

    #[tokio::test]
    async fn 泛型命令被实例化为_wry() {
        // NoopHostBridge 的 save_file 返回 None。
        let out = call("save_file_dialog", json!({ "defaultName": "a.sql" }))
            .await
            .unwrap();
        assert_eq!(out, json!(null));
    }

    #[tokio::test]
    async fn window_参数注入可用() {
        let out = call("set_window_theme", json!({ "theme": "dark" }))
            .await
            .unwrap();
        assert_eq!(out, json!(null));
    }

    #[tokio::test]
    async fn 非_result_返回值直接序列化() {
        assert_eq!(
            call("list_app_types", json!({})).await.unwrap(),
            json!(["claude", "codex", "opencode"])
        );
        assert_eq!(call("is_ready", json!({})).await.unwrap(), json!(true));
        assert_eq!(call("ping", json!({})).await.unwrap(), json!(null));
    }

    #[tokio::test]
    async fn 自定义_display_错误被转成字符串() {
        assert_eq!(
            call("find_thing", json!({ "key": "ok" })).await.unwrap(),
            json!("found")
        );
        assert_eq!(
            call("find_thing", json!({ "key": "zzz" }))
                .await
                .unwrap_err(),
            "未找到: zzz"
        );
    }

    #[tokio::test]
    async fn 结构体入参反序列化() {
        let out = call(
            "upsert_provider",
            json!({ "payload": { "id": "x", "enabled": false }, "dryRun": true }),
        )
        .await
        .unwrap();
        assert_eq!(out, json!("sidecar:x:false:true"));
    }

    #[tokio::test]
    async fn 未知命令报错而非_panic() {
        let err = call("no_such_command", json!({})).await.unwrap_err();
        assert!(err.contains("未知命令"), "{err}");
    }

    #[tokio::test]
    async fn 未注册状态报错而非_panic() {
        // 不 manage 任何状态，直接调用需要 State 的命令。
        let app = AppHandle::noop();
        let handlers = tauri::generate_handler![commands::read_state_label];
        let err = handlers
            .dispatch("read_state_label", RpcContext::new(app), json!({}))
            .await
            .unwrap_err();
        assert!(err.contains("未注册"), "{err}");
    }

    #[tokio::test]
    async fn emit_经由_eventsink_送出() {
        let sink = Arc::new(RecordingSink(std::sync::Mutex::new(Vec::new())));
        let app = AppHandle::new(sink.clone(), Arc::new(tauri::NoopHostBridge));
        let handlers = tauri::generate_handler![commands::switch_and_emit];
        handlers
            .dispatch(
                "switch_and_emit",
                RpcContext::new(app),
                json!({ "target": "prov-9" }),
            )
            .await
            .unwrap();
        let events = sink.0.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "provider-switched");
        assert_eq!(events[0].1, json!("prov-9"));
    }

    #[tokio::test]
    async fn opener_未实现宿主时返回错误而非_panic() {
        let err = call(
            "open_external",
            json!({ "url": "https://example.com" }),
        )
        .await
        .unwrap_err();
        assert!(!err.is_empty());
    }

    #[tokio::test]
    async fn store_落盘并读回() {
        let dir = std::env::temp_dir().join("ccs-shim-selftest");
        std::fs::create_dir_all(&dir).unwrap();
        tauri::set_store_base_dir(&dir);
        let out = call("store_roundtrip", json!({ "key": "k1" }))
            .await
            .unwrap();
        assert_eq!(out, json!("v1"));
    }
}
