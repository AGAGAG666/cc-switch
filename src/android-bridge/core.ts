/** `@tauri-apps/api/core` 的 Android 替身。 */
export { invoke } from "./runtime";

/**
 * 桌面用它把本地文件路径转成 `asset://` 供 webview 读取。本移植的 WebView 不挂
 * 自定义协议，直接原样返回；当前前端没有用点（保留以防 rebase 引入）。
 */
export function convertFileSrc(filePath: string, _protocol = "asset"): string {
  return filePath;
}
