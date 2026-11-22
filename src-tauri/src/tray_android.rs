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

/// Android has no desktop tray navigation or tray-status badge.
#[tauri::command]
pub fn take_tray_navigation() -> Option<serde_json::Value> {
    None
}

#[tauri::command]
pub fn tray_app_page_seen(_app: tauri::AppHandle, _app_type: String) {}

pub fn schedule_tray_status_check(_app: &tauri::AppHandle) {}

use crate::app_config::AppType;
use crate::provider::Provider;
use crate::provider_kind::ProviderKind;

fn is_official_account(app: &AppType, provider: &Provider) -> bool {
    provider.has_official_category()
        || (*app == AppType::Codex && crate::proxy::providers::is_codex_official_provider(provider))
}

/// Codex / Grok Build 配置里当前供应商的 `wire_api`；TOML 读不懂时退回唯一的一条赋值。
fn codex_wire_api(config_text: &str) -> Option<String> {
    if let Ok(doc) = config_text.parse::<toml::Value>() {
        return crate::codex_config::active_provider_str(&doc, "wire_api").map(str::to_string);
    }
    let mut found = config_text.lines().filter_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "wire_api").then(|| {
            value
                .trim()
                .trim_matches(|c| c == '"' || c == '\'')
                .to_string()
        })
    });
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

/// 这家在这个应用下必须经过路由服务才能用（直连写进去也用不了）。和前端
/// `providerNeedsRouting`（`src/utils/providerCapabilities.ts`）是同一条规则，改一边要改另一边。
pub(crate) fn provider_needs_routing(app: &AppType, provider: &Provider) -> bool {
    if is_official_account(app, provider) {
        return false;
    }
    let meta = provider.meta.as_ref();
    let managed_oauth = ProviderKind::declared(provider).is_some();
    let full_url = meta.and_then(|meta| meta.is_full_url) == Some(true);
    let api_format = meta
        .and_then(|meta| meta.api_format.as_deref())
        .filter(|fmt| !fmt.is_empty());
    match app {
        AppType::ClaudeDesktop => {
            managed_oauth
                || meta
                    .and_then(|meta| meta.claude_desktop_mode.as_ref())
                    .is_some_and(|mode| *mode == crate::provider::ClaudeDesktopMode::Proxy)
        }
        AppType::Claude => {
            managed_oauth || full_url || api_format.is_some_and(|f| f != "anthropic")
        }
        AppType::Codex | AppType::GrokBuild => {
            managed_oauth
                || full_url
                || matches!(api_format, Some("openai_chat" | "anthropic"))
                || provider
                    .settings_config
                    .get("config")
                    .and_then(|config| config.as_str())
                    .and_then(codex_wire_api)
                    .is_some_and(|wire_api| {
                        crate::codex_config::is_chat_wire_api(&wire_api)
                            || crate::codex_config::is_anthropic_wire_api(&wire_api)
                    })
        }
        _ => false,
    }
}


pub fn apply_tray_style(_app: &tauri::AppHandle) {}

pub(crate) fn routing_in_use(state: &crate::store::AppState) -> bool {
    state.proxy_service.running_now() == Some(true) && crate::mode::current::any_proxy()
}

pub(crate) fn tray_language() -> String {
    crate::settings::get_settings().language.unwrap_or_else(|| {
        let locale = sys_locale::get_locale().unwrap_or_default().to_lowercase();
        if locale.starts_with("zh") { if locale.contains("tw") || locale.contains("hant") { "zh-TW" } else { "zh" } } else { "en" }.to_string()
    })
}
