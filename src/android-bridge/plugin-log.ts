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

/**
 * 上游按 `error(message, { file: "frontend" })` 调用（见 lib/frontendLogger.ts），
 * 与真实插件的 `LogOptions` 第二参一致，这里把它并进行首以便排查来源。
 */
export interface LogOptions {
  file?: string;
  line?: number;
  keyValues?: Record<string, string | undefined>;
}

function withOrigin(message: string, options?: LogOptions): string {
  if (!options) return message;
  const bits = [options.file, options.line != null ? String(options.line) : undefined]
    .filter(Boolean)
    .join(":");
  const kv = options.keyValues
    ? Object.entries(options.keyValues)
        .filter(([, v]) => v != null)
        .map(([k, v]) => `${k}=${v}`)
        .join(" ")
    : "";
  const suffix = [bits, kv].filter(Boolean).join(" ");
  return suffix ? `${message} (${suffix})` : message;
}

export async function error(
  message: string,
  options?: LogOptions,
): Promise<void> {
  record("error", withOrigin(message, options));
}

export async function warn(
  message: string,
  options?: LogOptions,
): Promise<void> {
  record("warn", withOrigin(message, options));
}

export async function info(
  message: string,
  options?: LogOptions,
): Promise<void> {
  record("info", withOrigin(message, options));
}

export async function debug(
  message: string,
  options?: LogOptions,
): Promise<void> {
  record("debug", withOrigin(message, options));
}

export async function trace(
  message: string,
  options?: LogOptions,
): Promise<void> {
  record("trace", withOrigin(message, options));
}
