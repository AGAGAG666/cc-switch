//! `arboard` 的 Termux 替身。
//!
//! 原版用点只有一处（`commands/misc.rs:43` 的 `copy_text_to_clipboard`）：
//! ```ignore
//! let mut clipboard = arboard::Clipboard::new().map_err(..)?;
//! clipboard.set_text(text).map_err(..)?;
//! ```
//! 桌面版走 X11/Win32/AppKit；Android 上没有这些，但 Termux 自带
//! `termux-clipboard-set` / `termux-clipboard-get` 可直通 Android 剪贴板，
//! 所以这里是**真实可用**的实现，不是空壳。

use std::io::Write;
use std::process::{Command, Stdio};

/// 复刻 `arboard::Error`（调用点只用 `Display`）。
#[derive(Debug)]
pub struct Error(String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// 复刻 `arboard::Clipboard`。
pub struct Clipboard {
    setter: &'static str,
    getter: &'static str,
}

impl Clipboard {
    /// 原版会连接系统剪贴板服务；这里检查 termux-api 工具是否可用。
    pub fn new() -> Result<Self, Error> {
        let setter = "termux-clipboard-set";
        let getter = "termux-clipboard-get";
        if which(setter) {
            Ok(Self { setter, getter })
        } else {
            Err(Error(format!(
                "{setter} 不可用；请安装 Termux:API 应用与 termux-api 包"
            )))
        }
    }

    /// 写入剪贴板。文本经 stdin 传入，避免出现在进程参数里。
    pub fn set_text(&mut self, text: impl Into<String>) -> Result<(), Error> {
        let text = text.into();
        let mut child = Command::new(self.setter)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error(format!("启动 {} 失败: {e}", self.setter)))?;

        child
            .stdin
            .as_mut()
            .ok_or_else(|| Error("无法写入剪贴板进程 stdin".into()))?
            .write_all(text.as_bytes())
            .map_err(|e| Error(format!("写入剪贴板失败: {e}")))?;
        // 必须先关闭 stdin，否则 termux-clipboard-set 会一直等输入。
        drop(child.stdin.take());

        let out = child
            .wait_with_output()
            .map_err(|e| Error(format!("等待剪贴板进程失败: {e}")))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(Error(format!(
                "{} 退出码 {:?}: {}",
                self.setter,
                out.status.code(),
                String::from_utf8_lossy(&out.stderr).trim()
            )))
        }
    }

    /// 读取剪贴板。原版此方法在 cc-switch 未被使用，但补上以防上游新增调用。
    pub fn get_text(&mut self) -> Result<String, Error> {
        let out = Command::new(self.getter)
            .output()
            .map_err(|e| Error(format!("启动 {} 失败: {e}", self.getter)))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).to_string())
        } else {
            Err(Error(format!(
                "{} 退出码 {:?}",
                self.getter,
                out.status.code()
            )))
        }
    }
}

/// PATH 查找，避免依赖 `which` crate。
fn which(bin: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let p = dir.join(bin);
        p.is_file()
    })
}
