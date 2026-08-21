/** `@tauri-apps/api/core` 的 Android 替身。 */
export { invoke } from "./runtime";

/**
 * 上游用 `isTauri()` 决定是否订阅原生窗口事件。Android WebView 虽然没有桌面
 * Tauri runtime，但已经由 sidecar bridge 提供等价 IPC/窗口能力，因此应返回 true。
 */
export function isTauri(): boolean {
  return true;
}

/**
 * 桌面用它把本地文件路径转成 `asset://` 供 webview 读取。本移植的 WebView 不挂
 * 自定义协议，直接原样返回；当前前端没有用点（保留以防 rebase 引入）。
 */
export function convertFileSrc(filePath: string, _protocol = "asset"): string {
  return filePath;
}
