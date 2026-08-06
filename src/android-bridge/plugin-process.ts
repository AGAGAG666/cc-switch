/**
 * `@tauri-apps/plugin-process` 的 Android 替身。
 *
 * 桌面 `exit(code)` 结束整个进程。Android 上「进程」有两半：WebView 页面在
 * ZeroTermux 里，sidecar 是独立进程。这里先请宿主收拾（关页面 + 停 sidecar），
 * 宿主缺失时退化成让页面自己变成一块说明，避免用户面对一个已经不可用的 UI。
 */
import { requestHostAction } from "./runtime";

export async function exit(code = 0): Promise<void> {
  await requestHostAction("exit", String(code));
}

export async function relaunch(): Promise<void> {
  await requestHostAction("restart");
}
