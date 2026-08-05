//! `auto-launch` 的 Android 替身。
//!
//! 原版用点只在 `src/auto_launch.rs`，形态是：
//! ```ignore
//! let auto_launch = AutoLaunchBuilder::new()
//!     .set_app_name(app_name)
//!     .set_app_path(&app_path.to_string_lossy())
//!     .build()?;
//! auto_launch.enable()? / .disable()? / .is_enabled()?
//! ```
//! 桌面版写注册表 / XDG autostart / AppleScript login item。Android 上
//! 「开机自启」不是应用能自行设置的东西（需要 RECEIVE_BOOT_COMPLETED 广播
//! 接收器，由 ZeroTermux 宿主决定），所以这里如实报告「不支持」，
//! 让前端开关显示为关闭且切换时给出明确原因，而不是假装成功。

/// 复刻 `auto_launch::Error`（调用点只用 `Display`）。
#[derive(Debug)]
pub struct Error(String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// 复刻 `auto_launch::AutoLaunch`。
pub struct AutoLaunch {
    app_name: String,
}

impl AutoLaunch {
    pub fn enable(&self) -> Result<(), Error> {
        Err(Error(format!(
            "Android 不支持由应用自行设置开机自启（{}）；请在 ZeroTermux 中配置启动项",
            self.app_name
        )))
    }

    pub fn disable(&self) -> Result<(), Error> {
        // 本就没有开启过，视为已处于关闭状态。
        Ok(())
    }

    pub fn is_enabled(&self) -> Result<bool, Error> {
        Ok(false)
    }
}

/// 复刻 `auto_launch::AutoLaunchBuilder`。
///
/// 保持原版的 `&mut self -> &mut Self` 链式签名，使调用点
/// `AutoLaunchBuilder::new().set_app_name(..).set_app_path(..).build()` 原样编译。
#[derive(Default)]
pub struct AutoLaunchBuilder {
    app_name: Option<String>,
    #[allow(dead_code)]
    app_path: Option<String>,
}

impl AutoLaunchBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_app_name(&mut self, name: &str) -> &mut Self {
        self.app_name = Some(name.to_string());
        self
    }

    pub fn set_app_path(&mut self, path: &str) -> &mut Self {
        self.app_path = Some(path.to_string());
        self
    }

    pub fn set_use_launch_agent(&mut self, _use_agent: bool) -> &mut Self {
        self
    }

    pub fn set_args(&mut self, _args: &[impl AsRef<str>]) -> &mut Self {
        self
    }

    pub fn build(&self) -> Result<AutoLaunch, Error> {
        let app_name = self
            .app_name
            .clone()
            .ok_or_else(|| Error("未设置 app_name".into()))?;
        Ok(AutoLaunch { app_name })
    }
}
