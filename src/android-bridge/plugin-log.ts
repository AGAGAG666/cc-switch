/**
 * `@tauri-apps/plugin-log` 的 Android 替身。
 *
 * 桌面靠 tauri-plugin-log 把前端日志转写进 Rust 的日志文件。sidecar 没有等价的
 * 内建命令（命令表里不存在任何 frontend log 命令），所以这里不能凭空 invoke 一个
 * 不存在的命令——那只会在 404 上白烧一次往返。
 *
 * 折中做法：写 WebView 控制台（宿主 WebChromeClient 会把它接进 logcat），
 * 同时在内存里留一份环形缓冲，方便「关于」页或问题排查时一次性导出。
 */
const RING_CAPACITY = 200;
const ring: string[] = [];

function record(level: string, message: string): void {
  const line = `${new Date().toISOString()} [${level}] ${message}`;
  ring.push(line);
  if (ring.length > RING_CAPACITY) ring.shift();

  if (level === "error") console.error("[frontend]", message);
  else if (level === "warn") console.warn("[frontend]", message);
  else console.log(`[frontend/${level}]`, message);
}

/** 供宿主或调试面板取最近日志。 */
export function recentFrontendLogs(): string[] {
  return [...ring];
}

export async function error(message: string): Promise<void> {
  record("error", message);
}

export async function warn(message: string): Promise<void> {
  record("warn", message);
}

export async function info(message: string): Promise<void> {
  record("info", message);
}

export async function debug(message: string): Promise<void> {
  record("debug", message);
}

export async function trace(message: string): Promise<void> {
  record("trace", message);
}
