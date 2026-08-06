/**
 * `@tauri-apps/plugin-updater` 的 Android 替身。
 *
 * 桌面靠 Tauri updater 拉签名清单自更新。Android 版是 ZeroTermux 的内嵌页面，
 * 版本由宿主 APK 决定，前端无权替换自己的实现，所以这里如实报告「无更新」，
 * 与 Rust 侧 `updater-shim.check() -> Ok(None)` 保持同一结论。
 */
export interface Update {
  version: string;
  notes?: string;
  date?: string;
  downloadAndInstall(
    onEvent?: (event: { event: string; data?: unknown }) => void,
  ): Promise<void>;
}

export async function check(_options?: unknown): Promise<Update | null> {
  return null;
}
