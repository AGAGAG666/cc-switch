//! sidecar 的日志后端。
//!
//! 桌面用 `tauri-plugin-log`（写文件 + stdout + webview）。Android 上 sidecar 由
//! `CcsProxyService` 以子进程启动，stderr 已被宿主接到 logcat，所以最简可靠的
//! 做法是直接写 stderr，不引入额外依赖，也不与宿主抢文件句柄。
//!
//! stdout **保留给启动握手那一行 JSON**，因此日志一律走 stderr。

use std::io::Write;

struct StderrLogger;

impl log::Log for StderrLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        // 实际过滤交给 `log::set_max_level`（数据库里的 LogConfig 会动态调整）。
        true
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let mut err = std::io::stderr().lock();
        let _ = writeln!(
            err,
            "[{}] {} {}: {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
            record.level(),
            record.target(),
            record.args()
        );
    }

    fn flush(&self) {
        let _ = std::io::stderr().flush();
    }
}

static LOGGER: StderrLogger = StderrLogger;

/// 安装 stderr logger。重复调用安全（第二次返回 Err，忽略）。
pub fn init(default_level: log::LevelFilter) {
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(default_level);
    }
}
