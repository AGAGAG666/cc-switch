//! Android sidecar 入口。
//!
//! 桌面版的 `run()` 是 1500 行 Tauri bootstrap（窗口 / 托盘 / 单实例 / 深链接 /
//! 插件注册）。Android 上宿主是 ZeroTermux 的 WebView，这里只需要把同一批命令
//! 与同一批状态挂到一个本地 HTTP 面上：
//!
//! | 原版                              | sidecar                       |
//! |-----------------------------------|-------------------------------|
//! | `invoke(cmd, args)` 走 Tauri IPC  | `POST /rpc/{cmd}`（JSON body）|
//! | `listen(event, cb)`               | `GET /events`（SSE 广播）      |
//! | `app.manage(state)`               | 同名 `manage`（shim StateMap） |
//! | opener / dialog / process 插件     | `HostBridge` -> Java 层        |
//!
//! 命令表与桌面**共用** `cc_switch_generate_handler!`，杜绝两端漂移。
//!
//! 安全性：监听地址固定 `127.0.0.1`，仅本机可达；且要求每个请求带
//! `X-CCS-Token`（启动时生成、通过 stdout 首行交给宿主）。Android 上同一台设备
//! 的其它 app 理论上能连 loopback，token 用于阻断这种越权调用。

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::extract::{Path, State as AxumState};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, Sse};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use tauri::rpc::{Handlers, RpcContext};
use tauri::{AppHandle, EventSink, HostBridge, Manager, Wry};
use tokio::sync::broadcast;

use crate::store::AppState;

/// 请求头里的鉴权字段名。
const TOKEN_HEADER: &str = "x-ccs-token";

/// SSE 广播容量。前端断线重连期间的事件允许丢弃（前端会重新拉取快照），
/// 因此用有界 channel 而非无界，避免宿主长时间后台时内存无上限增长。
const EVENT_CHANNEL_CAPACITY: usize = 256;

// ============================================================
// 事件汇：emit -> SSE
// ============================================================

/// 一条待推送的事件。
#[derive(Clone, Debug, serde::Serialize)]
struct SidecarEvent {
    event: String,
    payload: Value,
}

/// 把 `app.emit(name, payload)` 转成 SSE 广播。
struct BroadcastSink {
    tx: broadcast::Sender<SidecarEvent>,
}

impl EventSink for BroadcastSink {
    fn dispatch(&self, event: &str, payload: Value) {
        // 无订阅者时 send 返回 Err，属正常情况（WebView 未打开）。
        let _ = self.tx.send(SidecarEvent {
            event: event.to_string(),
            payload,
        });
    }
}

// ============================================================
// 宿主桥：opener / dialog / process
// ============================================================

/// 宿主动作请求，序列化后经 SSE 的 `host-action` 事件交给 Java 层执行。
///
/// 为什么不直接在 Rust 侧起 Intent：sidecar 是纯 native 进程，没有
/// `Context`，无法调用 `startActivity`。ZeroTermux 侧收到事件后转 Intent。
struct BridgeToHost {
    tx: broadcast::Sender<SidecarEvent>,
}

impl BridgeToHost {
    fn post(&self, action: &str, data: Value) {
        let _ = self.tx.send(SidecarEvent {
            event: "host-action".to_string(),
            payload: json!({ "action": action, "data": data }),
        });
    }
}

impl HostBridge for BridgeToHost {
    fn open_url(&self, url: &str) -> Result<(), String> {
        self.post("openUrl", json!({ "url": url }));
        Ok(())
    }

    fn open_path(&self, path: &str) -> Result<(), String> {
        self.post("openPath", json!({ "path": path }));
        Ok(())
    }

    fn pick_folder(&self) -> Option<String> {
        // 目录选择是同步返回语义，跨进程异步拿不回结果。
        // Android 上前端改用「手输路径 + 校验」，这里明确返回 None（等价用户取消）。
        log::warn!("sidecar 不支持同步目录选择，返回取消");
        None
    }

    fn restart(&self) {
        self.post("restart", json!({}));
    }

    fn exit(&self, code: i32) {
        self.post("exit", json!({ "code": code }));
    }
}

// ============================================================
// HTTP 面
// ============================================================

struct Shared {
    app: AppHandle<Wry>,
    handlers: Handlers,
    events: broadcast::Sender<SidecarEvent>,
    token: String,
}

fn authorized(shared: &Shared, headers: &HeaderMap) -> bool {
    headers
        .get(TOKEN_HEADER)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == shared.token)
}

/// `POST /rpc/{cmd}`，body 为 `invoke` 的参数对象。
async fn rpc(
    AxumState(shared): AxumState<Arc<Shared>>,
    Path(cmd): Path<String>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> impl IntoResponse {
    if !authorized(&shared, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "无效的 X-CCS-Token" })),
        );
    }

    let args = body.map(|Json(v)| v).unwrap_or_else(|| json!({}));
    let ctx = RpcContext::new(shared.app.clone());

    match shared.handlers.dispatch(&cmd, ctx, args).await {
        Ok(value) => (StatusCode::OK, Json(json!({ "ok": true, "data": value }))),
        Err(msg) => {
            // 与 Tauri IPC 一致：命令返回 Err 是业务错误，不是传输层错误。
            // 用 200 + ok:false 让前端 shim 统一 reject，避免和 401/404 混淆。
            log::debug!("命令 {cmd} 失败: {msg}");
            (
                StatusCode::OK,
                Json(json!({ "ok": false, "error": msg })),
            )
        }
    }
}

/// `GET /events`：SSE。对应前端 `listen()`。
async fn events(
    AxumState(shared): AxumState<Arc<Shared>>,
    headers: HeaderMap,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>>, StatusCode> {
    if !authorized(&shared, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let mut rx = shared.events.subscribe();
    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    let data = serde_json::to_string(&ev.payload)
                        .unwrap_or_else(|_| "null".to_string());
                    yield Ok(Event::default().event(ev.event).data(data));
                }
                // 订阅者跟不上：跳过丢失的事件继续收，不断流。
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    log::warn!("SSE 订阅者滞后，丢弃 {n} 条事件");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default()))
}

/// `GET /health`：宿主用来判断 sidecar 是否就绪（不需要 token）。
async fn health(AxumState(shared): AxumState<Arc<Shared>>) -> impl IntoResponse {
    Json(json!({
        "ok": true,
        "commands": shared.handlers.len(),
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// `GET /rpc`：已注册命令清单，用于两端命令表一致性自检。
async fn command_names(
    AxumState(shared): AxumState<Arc<Shared>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !authorized(&shared, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "无效的 X-CCS-Token" })),
        );
    }
    (
        StatusCode::OK,
        Json(json!({ "count": shared.handlers.len(), "names": shared.handlers.names() })),
    )
}

// ============================================================
// 启动
// ============================================================

/// 生成 32 hex 字符的请求 token。
fn new_token() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// 初始化状态并启动本地 HTTP 面。阻塞直到进程退出。
///
/// `port` 为 0 时由内核分配；真实端口与 token 以一行 JSON 打到 stdout，
/// 宿主（`CcsProxyService`）解析后交给 WebView。
pub fn run_sidecar(port: u16) -> Result<(), String> {
    // logger 必须先装，否则 Database::init / 迁移阶段的日志会被丢弃。
    // 真正的级别稍后由 init_states 依据数据库里的 LogConfig 覆盖。
    crate::android_log::init(log::LevelFilter::Info);
    crate::panic_hook::setup_panic_hook();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("创建 tokio runtime 失败: {e}"))?;

    runtime.block_on(async move { serve(port).await })
}

async fn serve(port: u16) -> Result<(), String> {
    let (tx, _rx) = broadcast::channel::<SidecarEvent>(EVENT_CHANNEL_CAPACITY);

    let app: AppHandle<Wry> = AppHandle::new(
        Arc::new(BroadcastSink { tx: tx.clone() }),
        Arc::new(BridgeToHost { tx: tx.clone() }),
    );

    // store 插件替身的落盘根目录，与桌面的 app config dir 语义对齐。
    tauri::set_store_base_dir(crate::config::get_app_config_dir());

    // usage_events 需要 AppHandle 才能推 `usage-log-recorded`。
    crate::usage_events::init(app.clone());

    init_states(&app)?;

    let handlers = Handlers::from_pairs(crate::cc_switch_generate_handler!());
    log::info!("已注册 {} 条命令", handlers.len());

    let token = new_token();
    let shared = Arc::new(Shared {
        app,
        handlers,
        events: tx,
        token: token.clone(),
    });

    let router = Router::new()
        .route("/health", get(health))
        .route("/events", get(events))
        .route("/rpc", get(command_names))
        .route("/rpc/:cmd", post(rpc))
        .with_state(shared);

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("绑定 {addr} 失败: {e}"))?;
    let local = listener
        .local_addr()
        .map_err(|e| format!("读取本地地址失败: {e}"))?;

    // 宿主握手：必须是 stdout 首行，且只打这一行结构化数据。
    println!(
        "{}",
        json!({ "ready": true, "port": local.port(), "token": token })
    );
    use std::io::Write;
    let _ = std::io::stdout().flush();

    log::info!("sidecar 监听 {local}");

    axum::serve(listener, router)
        .await
        .map_err(|e| format!("HTTP 服务退出: {e}"))
}

/// 复刻桌面 `run()` 里的状态注册（5 种 State）与数据库/代理初始化。
fn init_states(app: &AppHandle<Wry>) -> Result<(), String> {
    use crate::commands::skill::SkillServiceState;
    use crate::commands::{CodexOAuthState, CopilotAuthState, XaiOAuthState};
    use crate::proxy::providers::codex_oauth_auth::CodexOAuthManager;
    use crate::proxy::providers::copilot_auth::CopilotAuthManager;
    use crate::proxy::providers::xai_oauth_auth::XaiOAuthManager;
    use crate::services::SkillService;
    use tokio::sync::RwLock;

    let db = Arc::new(
        crate::database::Database::init().map_err(|e| format!("初始化数据库失败: {e}"))?,
    );

    // 数据库可用后立即应用持久化日志级别（与桌面一致，损坏配置 fail-closed 到 Info）。
    match db.get_log_config() {
        Ok(cfg) => log::set_max_level(cfg.to_level_filter()),
        Err(e) => {
            log::set_max_level(log::LevelFilter::Info);
            log::warn!("读取日志配置失败，回退 info: {e}");
        }
    }

    let app_state = AppState::new(db);
    app_state.proxy_service.set_app_handle(app.clone());

    if let Err(e) = crate::proxy::http_client::init(
        app_state
            .db
            .get_global_proxy_url()
            .ok()
            .flatten()
            .as_deref(),
    ) {
        log::error!("[GlobalProxy] 初始化失败: {e}");
    }

    app.manage(app_state);

    app.manage(SkillServiceState(Arc::new(SkillService::new())));

    let app_config_dir = crate::config::get_app_config_dir();
    app.manage(CopilotAuthState(Arc::new(RwLock::new(
        CopilotAuthManager::new(app_config_dir.clone()),
    ))));
    app.manage(CodexOAuthState(Arc::new(RwLock::new(
        CodexOAuthManager::new(app_config_dir.clone()),
    ))));
    app.manage(XaiOAuthState(Arc::new(RwLock::new(
        XaiOAuthManager::new(app_config_dir),
    ))));

    Ok(())
}
