//! 宿主抽象层（Android sidecar 移植用）。
//!
//! cc-switch 的核心逻辑（database / proxy / services / mcp / session_manager）
//! 基本不依赖 Tauri。真正的耦合集中在 `commands/` 层，用途只有 5 类：
//!   1. 发事件给前端        -> `emit_json`
//!   2. 打开外部 URL/目录   -> `open_external` / `open_path`
//!   3. 选择目录            -> `pick_directory`
//!   4. 应用生命周期        -> `restart` / `set_theme` / 轻量模式
//!   5. 托盘刷新            -> `refresh_tray`（Android 无托盘，空实现）
//!
//! 本 trait 把这 5 类抽象出来，使核心可同时服务：
//!   - 桌面：`TauriHost`（转发给 AppHandle，行为与原版一致）
//!   - Android：`AndroidHost`（事件转 SSE，打开动作交给 Java 层走 Intent）

use std::path::{Path, PathBuf};

/// 宿主能力。要求 object-safe，以便用 `Arc<dyn AppHost>` 传递。
///
/// 注意：`emit` 在原版是泛型 `Serialize`，泛型方法不是 object-safe，
/// 因此这里统一收敛到 `serde_json::Value`，并用 `emit` 默认方法保留原调用手感。
pub trait AppHost: Send + Sync {
    /// 向前端推送事件。事件名全集见 `events` 模块。
    fn emit_json(&self, event: &str, payload: serde_json::Value);

    /// 用系统默认方式打开一个 URL。
    fn open_external(&self, url: &str) -> Result<(), String>;

    /// 用系统文件管理器打开一个本地路径。
    fn open_path(&self, path: &Path) -> Result<(), String>;

    /// 弹出目录选择器。返回 `Ok(None)` 表示用户取消。
    fn pick_directory(&self) -> Result<Option<PathBuf>, String>;

    /// 重启应用。Android 上由 Java 层重启 sidecar。
    fn restart(&self) -> Result<(), String>;

    /// 刷新托盘菜单。Android 无托盘，默认空实现。
    fn refresh_tray(&self) {}

    /// 设置窗口主题。Android 由原生层接管，默认空实现。
    fn set_theme(&self, _theme: &str) -> Result<(), String> {
        Ok(())
    }

    /// 是否支持自更新。Android 由 ZeroTermux 自身管理，返回 false。
    fn supports_self_update(&self) -> bool {
        false
    }

    /// 便捷方法：保留原版 `app.emit("name", payload)` 的调用手感。
    fn emit<T: serde::Serialize>(&self, event: &str, payload: T)
    where
        Self: Sized,
    {
        match serde_json::to_value(payload) {
            Ok(v) => self.emit_json(event, v),
            Err(e) => log::error!("序列化事件 {event} 失败: {e}"),
        }
    }
}

/// 供非 Sized 场景（`Arc<dyn AppHost>`）使用的泛型发射助手。
pub fn emit_to<T: serde::Serialize>(host: &dyn AppHost, event: &str, payload: T) {
    match serde_json::to_value(payload) {
        Ok(v) => host.emit_json(event, v),
        Err(e) => log::error!("序列化事件 {event} 失败: {e}"),
    }
}

/// 前端监听的事件名全集（由原版源码穷举得到）。
pub mod events {
    pub const PROVIDER_SWITCHED: &str = "provider-switched";
    pub const USAGE_CACHE_UPDATED: &str = "usage-cache-updated";
    pub const USAGE_LOG_RECORDED: &str = "usage-log-recorded";
    pub const DEEPLINK_IMPORT: &str = "deeplink-import";
    pub const DEEPLINK_ERROR: &str = "deeplink-error";
    pub const PROXY_FLAGS_CHANGED: &str = "proxy-flags-changed";
    pub const PROFILE_APPLIED: &str = "profile-applied";
    pub const UNIVERSAL_PROVIDER_SYNCED: &str = "universal-provider-synced";
    pub const S3_SYNC_STATUS_UPDATED: &str = "s3-sync-status-updated";
    pub const WEBDAV_SYNC_STATUS_UPDATED: &str = "webdav-sync-status-updated";

    /// 全集，用于 SSE 通道注册与前端 shim 校验。
    pub const ALL: &[&str] = &[
        PROVIDER_SWITCHED,
        USAGE_CACHE_UPDATED,
        USAGE_LOG_RECORDED,
        DEEPLINK_IMPORT,
        DEEPLINK_ERROR,
        PROXY_FLAGS_CHANGED,
        PROFILE_APPLIED,
        UNIVERSAL_PROVIDER_SYNCED,
        S3_SYNC_STATUS_UPDATED,
        WEBDAV_SYNC_STATUS_UPDATED,
    ];
}

/// 不做任何事的宿主。用于单元测试与「无 UI」运行场景。
#[derive(Default)]
pub struct NullHost;

impl AppHost for NullHost {
    fn emit_json(&self, event: &str, _payload: serde_json::Value) {
        log::debug!("NullHost 丢弃事件: {event}");
    }
    fn open_external(&self, _url: &str) -> Result<(), String> {
        Err("当前宿主不支持打开外部链接".into())
    }
    fn open_path(&self, _path: &Path) -> Result<(), String> {
        Err("当前宿主不支持打开路径".into())
    }
    fn pick_directory(&self) -> Result<Option<PathBuf>, String> {
        Err("当前宿主不支持目录选择".into())
    }
    fn restart(&self) -> Result<(), String> {
        Err("当前宿主不支持重启".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct Recorder(Mutex<Vec<(String, serde_json::Value)>>);

    impl AppHost for Recorder {
        fn emit_json(&self, event: &str, payload: serde_json::Value) {
            self.0.lock().unwrap().push((event.to_string(), payload));
        }
        fn open_external(&self, _url: &str) -> Result<(), String> {
            Ok(())
        }
        fn open_path(&self, _path: &Path) -> Result<(), String> {
            Ok(())
        }
        fn pick_directory(&self) -> Result<Option<PathBuf>, String> {
            Ok(None)
        }
        fn restart(&self) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn 事件名全集不重复且非空() {
        let mut seen = std::collections::HashSet::new();
        for name in events::ALL {
            assert!(!name.is_empty());
            assert!(seen.insert(*name), "事件名重复: {name}");
        }
        assert_eq!(seen.len(), events::ALL.len());
    }

    #[test]
    fn 通过_trait_object_发射事件() {
        let rec = Arc::new(Recorder(Mutex::new(Vec::new())));
        let host: Arc<dyn AppHost> = rec.clone();
        emit_to(
            host.as_ref(),
            events::PROVIDER_SWITCHED,
            serde_json::json!({ "appType": "codex", "providerId": "p1" }),
        );
        let got = rec.0.lock().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, "provider-switched");
        assert_eq!(got[0].1["providerId"], "p1");
    }

    #[test]
    fn null_host_对不支持的能力返回错误() {
        let h = NullHost;
        assert!(h.open_external("https://example.com").is_err());
        assert!(h.pick_directory().is_err());
        assert!(!h.supports_self_update());
        h.refresh_tray(); // 不应 panic
    }
}
