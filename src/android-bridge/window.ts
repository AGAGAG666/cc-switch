/**
 * `@tauri-apps/api/window` 的 Android 替身。
 *
 * WebView 永远是「全屏、无边框、单窗口」。上游只用到 6 个方法（见 App.tsx），
 * 逐个给出 Android 上语义最接近的行为，而不是一律 no-op：
 *   - isMaximized -> 恒 true（前端据此隐藏「还原」按钮）
 *   - onResized   -> 转 window resize（旋转屏 / 输入法弹出时确实会触发）
 *   - minimize    -> 请宿主把任务移到后台
 *   - close       -> 请宿主关闭 cc-switch 页面
 *   - setDecorations / toggleMaximize -> 无对应概念，安静忽略
 */
import { requestHostAction } from "./runtime";

export type Theme = "light" | "dark";
export type UnlistenFn = () => void;

class AndroidWindow {
  readonly label = "main";

  async isMaximized(): Promise<boolean> {
    return true;
  }

  async isMinimized(): Promise<boolean> {
    return false;
  }

  async isFullscreen(): Promise<boolean> {
    return true;
  }

  async onResized(handler: () => void): Promise<UnlistenFn> {
    const wrapped = (): void => handler();
    window.addEventListener("resize", wrapped);
    return () => window.removeEventListener("resize", wrapped);
  }

  async setDecorations(_decorations: boolean): Promise<void> {}

  async toggleMaximize(): Promise<void> {}

  async maximize(): Promise<void> {}

  async unmaximize(): Promise<void> {}

  async minimize(): Promise<void> {
    await requestHostAction("minimize");
  }

  async close(): Promise<void> {
    await requestHostAction("close");
  }

  async setTitle(_title: string): Promise<void> {}

  async theme(): Promise<Theme> {
    return window.matchMedia?.("(prefers-color-scheme: dark)")?.matches
      ? "dark"
      : "light";
  }
}

const singleton = new AndroidWindow();

export function getCurrentWindow(): AndroidWindow {
  return singleton;
}

/** 旧名，防 rebase 引入。 */
export const appWindow = singleton;
