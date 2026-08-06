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
use std::path::{Component, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Query, State as AxumState};
use axum::http::{header, HeaderMap, HeaderValue, Response, StatusCode};
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
    /// payload 形状必须与前端 shim 的 `dispatchHostAction` 完全一致：
    /// `{ kind: "open-url" | "open-path" | "restart" | "exit", value?: string }`。
    /// 前端还有一条自己发起的同名链路（窗口按钮），两条链路共用同一组字段。
    fn post(&self, kind: &str, value: Option<String>) {
        let _ = self.tx.send(SidecarEvent {
            event: "host-action".to_string(),
            payload: match value {
                Some(v) => json!({ "kind": kind, "value": v }),
                None => json!({ "kind": kind }),
            },
        });
    }
}

impl HostBridge for BridgeToHost {
    fn open_url(&self, url: &str) -> Result<(), String> {
        self.post("open-url", Some(url.to_string()));
        Ok(())
    }

    fn open_path(&self, path: &str) -> Result<(), String> {
        self.post("open-path", Some(path.to_string()));
        Ok(())
    }

    fn pick_folder(&self) -> Option<String> {
        // 目录选择是同步返回语义，跨进程异步拿不回结果。
        // Android 上前端改用「手输路径 + 校验」，这里明确返回 None（等价用户取消）。
        log::warn!("sidecar 不支持同步目录选择，返回取消");
        None
    }

    fn restart(&self) {
        self.post("restart", None);
    }

    fn exit(&self, code: i32) {
        self.post("exit", Some(code.to_string()));
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
    /// 实际监听端口（注入前端握手用）。
    port: u16,
    /// 前端产物目录（`dist-android`）。`None` 时不托管静态站点，
    /// 宿主需自行提供页面并调 `window.__ccsSetHandshake`。
    webroot: Option<PathBuf>,
}

fn authorized(shared: &Shared, headers: &HeaderMap) -> bool {
    headers
        .get(TOKEN_HEADER)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == shared.token)
}

/// EventSource 无法附加自定义 header，SSE 只能把 token 放 query。
/// 仅回环监听 + 一次性随机 token，query 泄漏面等同 header。
fn authorized_with_query(shared: &Shared, headers: &HeaderMap, query: &TokenQuery) -> bool {
    if authorized(shared, headers) {
        return true;
    }
    query.token.as_deref() == Some(shared.token.as_str())
}

/// `?token=..` 查询参数。
#[derive(Debug, Default, serde::Deserialize)]
pub struct TokenQuery {
    #[serde(default)]
    pub token: Option<String>,
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

    // 未注册命令是「接口不存在」，必须与命令内部报错区分：
    // 前端 shim 靠 404 给出「未知命令」而不是把它当业务失败显示。
    let Some(handler) = shared.handlers.get(&cmd) else {
        log::warn!("未知命令: {cmd}");
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "ok": false, "error": format!("未知命令: {cmd}") })),
        );
    };

    let ctx = RpcContext::new(shared.app.clone());

    match handler(ctx, args).await {
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
    Query(query): Query<TokenQuery>,
    headers: HeaderMap,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>>, StatusCode> {
    if !authorized_with_query(&shared, &headers, &query) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let mut rx = shared.events.subscribe();
    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    // 必须用「无名事件 + data 里带事件名」：浏览器的 EventSource
                    // 对 `event: xxx` 命名事件只走 addEventListener("xxx")，
                    // 不触发 onmessage。前端 shim 是单一 onmessage 分发器，
                    // 所以这里把事件名塞进 data，由 shim 自己路由。
                    let data = serde_json::to_string(&json!({
                        "event": ev.event,
                        "payload": ev.payload,
                    }))
                    .unwrap_or_else(|_| "null".to_string());
                    yield Ok(Event::default().data(data));
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
// 静态站点（同源托管前端）
// ============================================================

/// 前端产物同源托管的理由：
///
/// 1. `file://` 下的 WebView 对 `fetch("http://127.0.0.1:PORT/...")` 属跨源，
///    Android WebView 默认 `allow-universal-access-from-file` 关闭，请求会被拦；
///    开这个开关等于给本地页面万能跨源权限，比同源托管危险得多。
/// 2. `EventSource` 在 `file://` 源下同样受同源策略限制。
/// 3. 同源后 `X-CCS-Token` 仍然生效，token 仅由本进程注入到 index.html，
///    其它 App 拿不到（它们即使能连回环也读不到 token）。
async fn static_asset(
    AxumState(shared): AxumState<Arc<Shared>>,
    Path(path): Path<String>,
) -> Response<Body> {
    serve_static(&shared, &path).await
}

async fn static_index(AxumState(shared): AxumState<Arc<Shared>>) -> Response<Body> {
    serve_static(&shared, "index.html").await
}

fn plain(status: StatusCode, msg: &str) -> Response<Body> {
    let mut res = Response::new(Body::from(msg.to_string()));
    *res.status_mut() = status;
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    res
}

/// 把 URL 路径安全地映射到 webroot 下的文件。
///
/// 只接受普通路径段：`..`、绝对根、Windows 前缀一律拒绝，防止宿主 WebView 里
/// 的任意页面通过 `/../../..` 读到 app 私有目录里的其它文件。
fn resolve_under(root: &std::path::Path, rel: &str) -> Option<PathBuf> {
    let mut out = root.to_path_buf();
    for comp in std::path::Path::new(rel).components() {
        match comp {
            Component::Normal(seg) => out.push(seg),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(out)
}

fn content_type_for(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "map" => "application/json; charset=utf-8",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

/// 注入到 index.html `<head>` 里的握手脚本。必须在任何业务脚本之前执行，
/// 这样 `runtime.ts` 的 `waitForHandshake()` 首次调用就能同步命中。
fn handshake_script(shared: &Shared) -> String {
    format!(
        "<script>window.__CCS_SIDECAR__={{\"port\":{},\"token\":\"{}\"}};</script>",
        shared.port, shared.token
    )
}

async fn serve_static(shared: &Shared, rel: &str) -> Response<Body> {
    let Some(root) = shared.webroot.as_ref() else {
        return plain(
            StatusCode::NOT_FOUND,
            "sidecar 未配置 --webroot，静态站点不可用",
        );
    };

    let rel = if rel.is_empty() { "index.html" } else { rel };
    let Some(mut target) = resolve_under(root, rel) else {
        return plain(StatusCode::BAD_REQUEST, "非法路径");
    };
    if target.is_dir() {
        target.push("index.html");
    }

    // SPA 回退：非资源请求（无扩展名或指向不存在的路由）交给 index.html。
    // cc-switch 前端用的是内存路由，实际只会命中 `/`，这里兜底以防未来加 router。
    if !target.is_file() {
        target = root.join("index.html");
        if !target.is_file() {
            return plain(StatusCode::NOT_FOUND, "index.html 不存在");
        }
    }

    let bytes = match tokio::fs::read(&target).await {
        Ok(b) => b,
        Err(e) => {
            log::warn!("读取静态文件失败 {}: {e}", target.display());
            return plain(StatusCode::INTERNAL_SERVER_ERROR, "读取静态文件失败");
        }
    };

    let ctype = content_type_for(&target);
    let is_index = ctype.starts_with("text/html");

    let body = if is_index {
        let html = String::from_utf8_lossy(&bytes).to_string();
        let script = handshake_script(shared);
        // `<head>` 之后立刻插入；没有 head 时退化为整体前置。
        match html.find("<head>") {
            Some(i) => {
                let at = i + "<head>".len();
                let mut out = String::with_capacity(html.len() + script.len());
                out.push_str(&html[..at]);
                out.push_str(&script);
                out.push_str(&html[at..]);
                Body::from(out)
            }
            None => Body::from(format!("{script}{html}")),
        }
    } else {
        Body::from(bytes)
    };

    let mut res = Response::new(body);
    res.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(ctype));
    // index.html 带一次性 token，绝不允许被 WebView 缓存复用到下一次启动。
    let cache = if is_index {
        "no-store"
    } else {
        "public, max-age=31536000, immutable"
    };
    res.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    res
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
///
/// `webroot` 指向前端产物目录（`dist-android`）。给了就同源托管站点，
/// 宿主只需 `loadUrl("http://127.0.0.1:{port}/")`，无需处理跨源与 token 注入。
pub fn run_sidecar(port: u16, webroot: Option<PathBuf>) -> Result<(), String> {
    // logger 必须先装，否则 Database::init / 迁移阶段的日志会被丢弃。
    // 真正的级别稍后由 init_states 依据数据库里的 LogConfig 覆盖。
    crate::android_log::init(log::LevelFilter::Info);
    crate::panic_hook::setup_panic_hook();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("创建 tokio runtime 失败: {e}"))?;

    runtime.block_on(async move { serve(port, webroot).await })
}

async fn serve(port: u16, webroot: Option<PathBuf>) -> Result<(), String> {
    let (tx, _rx) = broadcast::channel::<SidecarEvent>(EVENT_CHANNEL_CAPACITY);

    let app: AppHandle<Wry> = AppHandle::new(
        Arc::new(BroadcastSink { tx: tx.clone() }),
        Arc::new(BridgeToHost { tx: tx.clone() }),
    );

    // rustls 进程级 crypto provider。桌面在 `run()` 的 setup 里装（lib.rs），
    // Android 走 sidecar 不经过那条路径，必须在任何 TLS 握手之前自己装，
    // 否则 ClientConfig::builder() 会 panic（no process-level CryptoProvider）。
    let _ = rustls::crypto::ring::default_provider().install_default();

    // store 插件替身的落盘根目录，与桌面的 app config dir 语义对齐。
    tauri::set_store_base_dir(crate::config::get_app_config_dir());

    // 与桌面 setup 同序：先读 Store 里的 app_config_dir 覆盖，再让 panic_hook
    // 记住最终目录，保证崩溃日志和数据库落在同一个根下。
    crate::app_store::refresh_app_config_dir_override(&app);
    crate::panic_hook::init_app_config_dir(crate::config::get_app_config_dir());

    // usage_events 需要 AppHandle 才能推 `usage-log-recorded`。
    crate::usage_events::init(app.clone());

    init_states(&app)?;

    // `generate_handler!` 替身直接展开为 `Handlers::from_pairs(vec![..])`。
    let mut handlers: Handlers = crate::cc_switch_generate_handler!();
    // Android 专属补丁命令：桌面这两条能力由 Tauri 运行时内建提供，命令表里没有。
    handlers.extend(tauri::generate_handler![
        crate::commands::get_app_version,
        crate::commands::get_home_dir,
    ]);
    log::info!("已注册 {} 条命令", handlers.len());

    let token = new_token();

    let webroot = match webroot {
        Some(dir) if dir.is_dir() => Some(dir),
        Some(dir) => {
            // 不 fail-fast：宿主可能自带页面，只是没传/传错目录。记日志继续跑，
            // /rpc 面仍然可用，前端可由宿主注入握手。
            log::warn!("webroot 不是目录，静态站点关闭: {}", dir.display());
            None
        }
        None => None,
    };

    // 端口先绑定再建 Shared：index.html 注入的握手需要真实端口，
    // 而 port=0 时真实端口只有 bind 之后才知道。
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("绑定 {addr} 失败: {e}"))?;
    let local = listener
        .local_addr()
        .map_err(|e| format!("读取本地地址失败: {e}"))?;

    let has_webroot = webroot.is_some();
    let shared = Arc::new(Shared {
        app,
        handlers,
        events: tx,
        token: token.clone(),
        port: local.port(),
        webroot,
    });

    let mut router = Router::new()
        .route("/health", get(health))
        .route("/events", get(events))
        .route("/rpc", get(command_names))
        .route("/rpc/:cmd", post(rpc));
    if has_webroot {
        router = router
            .route("/", get(static_index))
            .route("/*path", get(static_asset));
    }
    let router = router.with_state(shared);

    // 宿主握手：必须是 stdout 首行，且只打这一行结构化数据。
    println!(
        "{}",
        json!({
            "ready": true,
            "port": local.port(),
            "token": token,
            "url": format!("http://127.0.0.1:{}/", local.port()),
            "webroot": has_webroot,
        })
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
