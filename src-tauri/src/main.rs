// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 在 Linux 上设置 WebKit 环境变量以解决 DMA-BUF 渲染问题
    // 某些 Linux 系统（如 Debian 13.2、Nvidia GPU）上 WebKitGTK 的 DMA-BUF 渲染器可能导致白屏/黑屏
    // 参考: https://github.com/tauri-apps/tauri/issues/9394
    #[cfg(target_os = "linux")]
    {
        if std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_err() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
        // 禁用 WebKitGTK 合成模式，规避 resize 时 webview 崩溃以及部分 Wayland
        // 合成器下的 surface 协商问题（整窗 UI 点击无响应、必须最大化-还原才能恢复）。
        // 参考: https://github.com/tauri-apps/tauri/issues/9394
        if std::env::var("WEBKIT_DISABLE_COMPOSITING_MODE").is_err() {
            std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        }

        // AppImage 的 GTK 启动钩子 (linuxdeploy-plugin-gtk.sh) 会无条件
        // `export GDK_BACKEND=x11` 强制走 XWayland，以规避历史上的 Wayland 崩溃
        // (tauri-apps/tauri#8541)。但在较新的 Wayland + NVIDIA 环境下，强制 XWayland
        // 反而使 WebKitGTK 的 webview 收不到指针事件（标题栏可点、网页内容点不动），
        // resize 后黑屏；改回原生 Wayland 即可解决，且该崩溃在 WebKitGTK 2.52 上已不复现。
        // 由于该钩子会覆盖用户预设的 GDK_BACKEND，这里提供一个钩子不会触碰的逃生开关：
        // 设置 CC_SWITCH_GDK_BACKEND=wayland 即可强制覆盖，默认行为保持不变（零回归）。
        if let Ok(backend) = std::env::var("CC_SWITCH_GDK_BACKEND") {
            if !backend.is_empty() {
                std::env::set_var("GDK_BACKEND", backend);
            }
        }
    }

    #[cfg(not(target_os = "android"))]
    cc_switch_lib::run();

    // Android 上没有 WebView 宿主窗口，本进程作为 sidecar 运行：
    // 只起本地 HTTP 面（/rpc、/events、/health）与代理服务，UI 由 ZeroTermux 的
    // WebView 承载。真实端口与访问 token 由 run_sidecar 以一行 JSON 打到 stdout。
    #[cfg(target_os = "android")]
    {
        let port = android_sidecar_port();
        if let Err(err) = cc_switch_lib::run_sidecar(port) {
            eprintln!("cc-switch sidecar 启动失败: {err}");
            std::process::exit(1);
        }
    }
}

/// 解析 sidecar 监听端口。
///
/// 优先级：`--port <N>` / `--port=<N>` 命令行参数 > `CCS_SIDECAR_PORT` 环境变量 >
/// 0（交由内核分配，宿主从握手行读取真实端口）。无法解析的值一律回落到 0，
/// 避免宿主因参数拼写错误直接拿不到服务。
#[cfg(target_os = "android")]
fn android_sidecar_port() -> u16 {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix("--port=") {
            if let Ok(port) = value.parse::<u16>() {
                return port;
            }
        } else if arg == "--port" {
            if let Some(value) = args.next() {
                if let Ok(port) = value.parse::<u16>() {
                    return port;
                }
            }
        }
    }

    std::env::var("CCS_SIDECAR_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(0)
}
