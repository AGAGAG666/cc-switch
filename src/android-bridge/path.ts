/** `@tauri-apps/api/path` 的 Android 替身。 */
import { invoke } from "./runtime";

let cachedHome: string | null = null;

/** 走 sidecar 的 `get_home_dir`，避免前端硬编码 Android 的 HOME。 */
export async function homeDir(): Promise<string> {
  if (cachedHome) return cachedHome;
  const home = await invoke<string>("get_home_dir");
  cachedHome = home;
  return home;
}

/** 纯字符串拼接即可：目标平台只有 Android（POSIX 分隔符）。 */
export async function join(...parts: string[]): Promise<string> {
  const joined = parts
    .filter((p) => p.length > 0)
    .join("/")
    .replace(/\/{2,}/g, "/");
  return joined;
}
