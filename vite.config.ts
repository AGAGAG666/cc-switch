import fs from "node:fs";
import path from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { codeInspectorPlugin } from "code-inspector-plugin";

/**
 * Android 模式（`CCS_TARGET=android vite build`）。
 *
 * 前端源码一行不改，靠 alias 把 `@tauri-apps/*` 全部换成 `src/android-bridge/*`
 * 的替身：`invoke` 走 `POST http://127.0.0.1:<port>/rpc/<cmd>`，`listen` 走
 * `GET /events` 的 SSE。Rust 侧同一份命令表由 sidecar 进程提供。
 */
const isAndroid = process.env.CCS_TARGET === "android";

const bridge = (name: string) =>
  path.resolve(__dirname, `./src/android-bridge/${name}`);

/** Android 上没有 Tauri 运行时，这些模块必须整体替换。 */
const androidAlias: Record<string, string> = {
  "@tauri-apps/api/core": bridge("core.ts"),
  "@tauri-apps/api/event": bridge("event.ts"),
  "@tauri-apps/api/app": bridge("app.ts"),
  "@tauri-apps/api/path": bridge("path.ts"),
  "@tauri-apps/api/window": bridge("window.ts"),
  "@tauri-apps/plugin-process": bridge("plugin-process.ts"),
  "@tauri-apps/plugin-updater": bridge("plugin-updater.ts"),
  "@tauri-apps/plugin-dialog": bridge("plugin-dialog.ts"),
  "@tauri-apps/plugin-log": bridge("plugin-log.ts"),
};

export default defineConfig(({ command }) => ({
  root: "src",
  plugins: [
    command === "serve" &&
      !isAndroid &&
      codeInspectorPlugin({
        bundler: "vite",
      }),
    react(),
  ].filter(Boolean),
  base: "./",
  define: isAndroid
    ? {
        // app.ts 的兜底版本号（sidecar 未就绪时也要能显示「关于」）
        __CCS_APP_VERSION__: JSON.stringify(
          JSON.parse(
            fs.readFileSync(path.resolve(__dirname, "package.json"), "utf-8"),
          ).version ?? "0.0.0",
        ),
      }
    : {},
  build: {
    outDir: isAndroid ? "../dist-android" : "../dist",
    emptyOutDir: true,
    // WebView 是 Chromium，且只有一个 HTML 入口，拆包收益不大；
    // 单文件能减少 file:// 下的请求数，首屏更快。
    chunkSizeWarningLimit: 4096,
  },
  server: {
    port: 3000,
    strictPort: true,
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
      ...(isAndroid ? androidAlias : {}),
    },
  },
  clearScreen: false,
  envPrefix: ["VITE_", "TAURI_"],
}));
