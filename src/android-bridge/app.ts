/** `@tauri-apps/api/app` 的 Android 替身。 */
import { invoke } from "./runtime";

/**
 * 桌面走 Tauri 内建命令读 tauri.conf.json 的版本号。sidecar 侧没有那条内建命令，
 * 改读我们自己的 `get_app_version`（Rust 侧用 `env!("CARGO_PKG_VERSION")`，
 * 与桌面同源）。取不到时回落到构建期注入的常量，保证「关于」页面不空白。
 */
export async function getVersion(): Promise<string> {
  try {
    return await invoke<string>("get_app_version");
  } catch {
    return __CCS_APP_VERSION__;
  }
}

export async function getName(): Promise<string> {
  return "cc-switch";
}
