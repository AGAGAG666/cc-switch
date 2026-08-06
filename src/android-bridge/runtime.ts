/**
 * Android sidecar 运行时桥。
 *
 * 桌面版前端通过 Tauri 的 IPC 调 Rust；Android 上 Rust 以独立进程（sidecar）跑在
 * 127.0.0.1 的一个随机端口上，前端跑在 ZeroTermux 的 WebView 里。宿主 Java 层在
 * 页面加载前注入 `window.__CCS_SIDECAR__ = { port, token }`，本模块据此把
 * `invoke()` 转成 `POST /rpc/{cmd}`，`listen()` 转成 `GET /events` 的 SSE 订阅。
 *
 * 所有请求都带 `X-CCS-Token`：sidecar 只监听回环地址，但同机其它 App 仍可访问
 * 回环，token 是必要的第二道闸。
 */

export interface SidecarHandshake {
  port: number;
  token: string;
}

declare global {
  interface Window {
    __CCS_SIDECAR__?: SidecarHandshake;
    /** 宿主注入：处理 sidecar 通过 SSE 发回的 host-action（开链接/重启等）。 */
    __CCS_HOST__?: {
      openUrl?: (url: string) => void;
      openPath?: (path: string) => void;
      pickFolder?: () => Promise<string | null>;
      restart?: () => void;
      exit?: (code: number) => void;
      /** 把 ZeroTermux 任务移到后台（对应桌面的最小化）。 */
      minimize?: () => void;
      /** 关闭 cc-switch 页面，回到终端。 */
      close?: () => void;
      toast?: (text: string) => void;
    };
    /**
     * 宿主通过 `addJavascriptInterface` 注入的同步能力面。
     * `@JavascriptInterface` 方法不能返回 Promise，所以目录选择走 id + 回调。
     */
    __CCS_NATIVE__?: {
      openUrl?: (url: string) => void;
      openPath?: (path: string) => void;
      pickFolder?: (requestId: number) => void;
      homeDir?: () => string;
      restart?: () => void;
      exit?: (code: number) => void;
      minimize?: () => void;
      close?: () => void;
      toast?: (text: string) => void;
    };
    /** 宿主完成目录选择后回调（由本模块安装）。 */
    __ccsResolveFolderPick?: (requestId: number, path: string | null) => void;
  }
}

type HostSurface = NonNullable<Window["__CCS_HOST__"]>;

let folderPickSeq = 0;
const folderPickWaiters = new Map<number, (path: string | null) => void>();

/**
 * 由 `__CCS_NATIVE__` 现场合成宿主面。
 *
 * 为什么需要这条回退路径：宿主原本在 `onPageFinished` 里 evaluateJavascript 装
 * `__CCS_HOST__`，那是页面 load 之后的时点。业务代码目前只在用户交互时才取宿主，
 * 时序上够用，但一旦将来有模块在挂载期就调（或 WebView 重载后尚未补装），
 * 就会静默拿到 undefined。`__CCS_NATIVE__` 是 addJavascriptInterface 绑定的，
 * 从脚本执行的第一行起就可用，据它现场合成可以彻底摆脱注入时序。
 */
function buildHostFromNative(): HostSurface | undefined {
  const n = window.__CCS_NATIVE__;
  if (!n) return undefined;

  if (!window.__ccsResolveFolderPick) {
    window.__ccsResolveFolderPick = (requestId, path) => {
      const resolve = folderPickWaiters.get(requestId);
      if (!resolve) return;
      folderPickWaiters.delete(requestId);
      resolve(path || null);
    };
  }

  return {
    openUrl: (url) => n.openUrl?.(url),
    openPath: (path) => n.openPath?.(path),
    pickFolder: () =>
      new Promise<string | null>((resolve) => {
        if (!n.pickFolder) {
          resolve(null);
          return;
        }
        const id = ++folderPickSeq;
        folderPickWaiters.set(id, resolve);
        try {
          n.pickFolder(id);
        } catch (e) {
          folderPickWaiters.delete(id);
          console.warn("[ccs] 宿主目录选择调用失败", e);
          resolve(null);
        }
      }),
    restart: () => n.restart?.(),
    exit: (code) => n.exit?.(code),
    minimize: () => n.minimize?.(),
    close: () => n.close?.(),
    toast: (text) => n.toast?.(text),
  };
}

/**
 * 取宿主能力面：优先用宿主已注入的 `__CCS_HOST__`，否则据 `__CCS_NATIVE__` 合成
 * 并缓存回 `window.__CCS_HOST__`，让后续调用与宿主注入的形态一致。
 */
function resolveHost(): HostSurface | undefined {
  const injected = window.__CCS_HOST__;
  if (injected) return injected;
  const synthesized = buildHostFromNative();
  if (synthesized) window.__CCS_HOST__ = synthesized;
  return synthesized;
}

let handshakeWaiters: Array<(h: SidecarHandshake) => void> = [];

/** 宿主也可以在页面加载后调这个函数补交握手信息。 */
export function setHandshake(h: SidecarHandshake): void {
  window.__CCS_SIDECAR__ = h;
  const waiters = handshakeWaiters;
  handshakeWaiters = [];
  for (const w of waiters) w(h);
}

// 暴露给 Java 层：evaluateJavascript("window.__ccsSetHandshake(...)")
(window as unknown as Record<string, unknown>).__ccsSetHandshake = setHandshake;

const HANDSHAKE_TIMEOUT_MS = 30_000;

/**
 * 等握手就绪。宿主先起 sidecar 再 load 页面，正常情况下同步就有值；
 * 但 WebView 有可能先于 sidecar 的 stdout 首行完成加载，所以这里允许等待。
 */
export function waitForHandshake(): Promise<SidecarHandshake> {
  const existing = window.__CCS_SIDECAR__;
  if (existing) return Promise.resolve(existing);

  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      handshakeWaiters = handshakeWaiters.filter((w) => w !== onReady);
      reject(
        new Error(
          "等待 cc-switch sidecar 握手超时：宿主未注入 window.__CCS_SIDECAR__",
        ),
      );
    }, HANDSHAKE_TIMEOUT_MS);

    function onReady(h: SidecarHandshake) {
      clearTimeout(timer);
      resolve(h);
    }
    handshakeWaiters.push(onReady);
  });
}

function baseUrl(h: SidecarHandshake): string {
  return `http://127.0.0.1:${h.port}`;
}

/** sidecar 的命令错误走 200 + { ok: false, error }，与传输层错误区分。 */
interface RpcEnvelope {
  ok: boolean;
  data?: unknown;
  error?: string;
}

/**
 * 与 `@tauri-apps/api/core` 的 `invoke` 同签名。
 *
 * 失败时 reject 一个字符串（Tauri 对 `Result<_, String>` 的行为一致），
 * 这样上游 `catch (e) { String(e) }` 那套错误展示逻辑无需改动。
 */
export async function invoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  const h = await waitForHandshake();

  let res: Response;
  try {
    res = await fetch(`${baseUrl(h)}/rpc/${encodeURIComponent(cmd)}`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-CCS-Token": h.token,
      },
      body: JSON.stringify(args ?? {}),
    });
  } catch (e) {
    throw `无法连接 cc-switch 服务（${cmd}）：${String(e)}`;
  }

  if (res.status === 404) {
    throw `未知命令：${cmd}`;
  }
  if (res.status === 401) {
    throw `cc-switch 服务鉴权失败（token 不匹配），请重启 cc-switch 页面`;
  }
  if (!res.ok) {
    throw `cc-switch 服务返回 ${res.status}（${cmd}）`;
  }

  const envelope = (await res.json()) as RpcEnvelope;
  if (!envelope.ok) {
    throw envelope.error ?? `命令 ${cmd} 执行失败`;
  }
  return envelope.data as T;
}

// ─── 事件（SSE）───────────────────────────────────────────────

export type UnlistenFn = () => void;

export interface Event<T> {
  event: string;
  id: number;
  payload: T;
}

type Handler = (event: Event<unknown>) => void;

const listeners = new Map<string, Set<Handler>>();
let source: EventSource | null = null;
let eventSeq = 0;

/** host-action 由宿主处理，不下发给业务组件。 */
const HOST_ACTION_EVENT = "host-action";

function dispatchHostAction(payload: unknown): void {
  const action = payload as { kind?: string; value?: string } | null;
  if (!action?.kind) return;
  const host = resolveHost();
  switch (action.kind) {
    case "open-url":
      host?.openUrl?.(action.value ?? "");
      break;
    case "open-path":
      host?.openPath?.(action.value ?? "");
      break;
    case "restart":
      host?.restart?.();
      break;
    case "exit":
      host?.exit?.(Number(action.value ?? 0));
      break;
    case "minimize":
      host?.minimize?.();
      break;
    case "close":
      host?.close?.();
      break;
    default:
      console.warn("[ccs] 未知 host-action", action);
  }
}

async function ensureSource(): Promise<void> {
  if (source) return;
  const h = await waitForHandshake();
  // EventSource 不能自定义 header，token 走 query（仅回环，且 sidecar 同时接受两种）。
  const url = `${baseUrl(h)}/events?token=${encodeURIComponent(h.token)}`;
  const es = new EventSource(url);
  source = es;

  es.onmessage = (msg) => {
    let parsed: { event?: string; payload?: unknown };
    try {
      parsed = JSON.parse(msg.data) as typeof parsed;
    } catch {
      return;
    }
    const name = parsed.event;
    if (!name) return;

    if (name === HOST_ACTION_EVENT) {
      dispatchHostAction(parsed.payload);
      return;
    }

    const set = listeners.get(name);
    if (!set?.size) return;
    const evt: Event<unknown> = {
      event: name,
      id: ++eventSeq,
      payload: parsed.payload,
    };
    for (const handler of [...set]) {
      try {
        handler(evt);
      } catch (e) {
        console.error(`[ccs] 事件 ${name} 处理失败`, e);
      }
    }
  };

  es.onerror = () => {
    // EventSource 自带重连；这里只降噪记录，不销毁实例，否则会丢订阅。
    console.warn("[ccs] SSE 连接中断，等待自动重连");
  };
}

/** 与 `@tauri-apps/api/event` 的 `listen` 同签名。 */
export async function listen<T>(
  event: string,
  handler: (event: Event<T>) => void,
): Promise<UnlistenFn> {
  await ensureSource();
  const set = listeners.get(event) ?? new Set<Handler>();
  listeners.set(event, set);
  const wrapped = handler as Handler;
  set.add(wrapped);
  return () => {
    set.delete(wrapped);
    if (set.size === 0) listeners.delete(event);
  };
}

/** `emit` 在本移植里没有实际用点，保留同签名以防前端后续引用。 */
export async function emit(event: string, payload?: unknown): Promise<void> {
  console.warn("[ccs] emit 在 Android 上不支持", event, payload);
}

// ─── 宿主动作（前端主动发起）─────────────────────────────────

export type HostActionKind =
  | "open-url"
  | "open-path"
  | "restart"
  | "exit"
  | "minimize"
  | "close";

/**
 * 前端直接请求宿主做一件事。
 *
 * 注意与 SSE 上的 `host-action` 区分：那条链路是 Rust 侧命令（比如 `open_url`
 * 命令、`app.restart()`）触发的，走 sidecar -> SSE -> 这里；本函数是前端自己
 * 发起的（窗口按钮、`exit()`），不必绕一圈 Rust。两条链路最终落到同一组
 * `window.__CCS_HOST__` 回调。
 */
export async function requestHostAction(
  kind: HostActionKind,
  value?: string,
): Promise<void> {
  const host = resolveHost();
  if (!host) {
    console.warn("[ccs] 宿主未注入 __CCS_HOST__/__CCS_NATIVE__，忽略动作", kind, value);
    return;
  }
  switch (kind) {
    case "open-url":
      host.openUrl?.(value ?? "");
      break;
    case "open-path":
      host.openPath?.(value ?? "");
      break;
    case "restart":
      host.restart?.();
      break;
    case "exit":
      host.exit?.(Number(value ?? 0));
      break;
    case "minimize":
      host.minimize?.();
      break;
    case "close":
      host.close?.();
      break;
  }
}

/**
 * 目录选择。sidecar 侧的 `pick_folder` 只能返回 None（Rust 进程没有 UI），
 * 所以由宿主的 SAF/文件选择器负责；宿主没实现时返回 null，前端会退回手输路径。
 */
export async function pickFolder(): Promise<string | null> {
  const host = resolveHost();
  if (!host?.pickFolder) return null;
  try {
    return await host.pickFolder();
  } catch (e) {
    console.warn("[ccs] 宿主目录选择失败", e);
    return null;
  }
}
