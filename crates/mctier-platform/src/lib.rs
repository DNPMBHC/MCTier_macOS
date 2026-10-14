//! MCTier 平台抽象契约（`mctier-platform`）。
//!
//! 本 crate 汇总所有操作系统行为差异的接缝（Seam）：
//! 1. [`WindowPolicy`] —— 窗口生命周期（关闭按钮语义、隐藏到后台、唤回恢复、最小化钩子）
//! 2. [`Notifier`] —— 后台运行通知与桌面提示
//! 3. [`Autostart`] —— 开机自启注册与查询
//! 4. [`Elevation`] —— 特权子进程（EasyTier / hosts）生命周期与退出清理
//! 5. [`AudioInputProvider`] —— 原生音频输入设备枚举
//! 6. [`PathRules`] —— 平台路径与保留文件名规则
//!
//! 各平台实现 crate（`mctier-platform-macos` / `mctier-platform-windows` /
//! `mctier-platform-linux`）各自给出零耦合的实现。

use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::{AppHandle, WebviewWindow};

/// 用户点击主窗口关闭按钮时的决策动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAction {
    /// 隐藏到后台（macOS 菜单栏 / Windows 系统托盘），应用继续运行。
    HideToBackground,
    /// 清理网络与录制资源后退出应用。
    Quit,
}

/// 窗口生命周期策略（平台接缝 #1 —— 最高优先级）。
///
/// - **macOS**：遵循 AppKit `NSWindow` 约定。关闭按钮隐藏窗口到菜单栏，直接调用
///   `window.hide()`，**不**做 `unminimize()`、**不**篡改 `always_on_top`、**不**起
///   300ms 延迟线程；最小化按钮走原生黄色按钮缩小到 Dock；点击 Dock / 菜单栏图标通过
///   `unminimize() + show() + set_focus()` 立即唤回。
/// - **Windows**：保留无边框透明窗（`WS_POPUP`）所需的全部规避逻辑（`unminimize` 前置、
///   置顶状态保存与恢复、`SetWindowSubclass` 拦截 `SC_MINIMIZE`、隐藏后释放前台窗口、
///   300ms 延迟托盘气泡通知）。
/// - **Linux**：标准桌面窗口隐藏与唤回。
pub trait WindowPolicy: Send + Sync {
    /// 当前平台「关闭时最小化到托盘/菜单栏」的缺省值（macOS 为 `true`，其余平台为 `false`）。
    fn default_close_to_background(&self) -> bool;

    /// 根据用户配置决定点击关闭按钮时的行为。
    fn close_action(&self, configured_close_to_tray: Option<bool>) -> CloseAction {
        let hide = configured_close_to_tray.unwrap_or_else(|| self.default_close_to_background());
        if hide {
            CloseAction::HideToBackground
        } else {
            CloseAction::Quit
        }
    }

    /// 在主窗口创建后安装平台专属的窗口钩子（如 Windows 的 `SC_MINIMIZE` 子类化）。
    fn install_window_hooks(&self, _window: &WebviewWindow) {}

    /// 将主窗口隐藏到后台（菜单栏/系统托盘）。
    ///
    /// 返回 `true` 表示本次确实从可见状态转入后台（调用方可据此决定是否发送后台通知）。
    fn hide_to_background(&self, window: &WebviewWindow, source: &str) -> Result<bool, String>;

    /// 将主窗口从后台或最小化状态唤回到前台。
    fn restore(&self, window: &WebviewWindow) -> Result<(), String>;
}

/// 后台运行通知与系统提示接口（平台接缝 #2）。
pub trait Notifier: Send + Sync {
    /// 当前平台默认的「最小化到后台」提示标题与正文模板（支持 `{shortcut}` 占位符）。
    fn default_background_notice(&self) -> (&'static str, &'static str);

    /// 发送主窗口已转入后台运行的通知。
    fn notify_running_in_background(&self, app: &AppHandle, title: &str, body: &str);
}

/// 开机自启接口（平台接缝 #3）。
pub trait Autostart: Send + Sync {
    fn set_enabled(&self, enable: bool) -> Result<(), String>;
    fn is_enabled(&self) -> bool;
}

/// 原生音频输入设备描述（供 `VoiceService` 跨平台枚举使用）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeAudioInput {
    pub device_id: String,
    pub label: String,
    pub is_default: bool,
    pub is_communications: bool,
}

/// 音频输入设备枚举提供者（平台接缝 #4）。
pub trait AudioInputProvider: Send + Sync {
    fn input_devices(&self) -> Vec<NativeAudioInput>;
}

/// 平台路径与文件名规则（平台接缝 #5）。
pub trait PathRules: Send + Sync {
    /// 是否为 Windows 保留设备名（CON/PRN/AUX/NUL/COM1..9/LPT1..9）。
    fn is_reserved_device_name(&self, name: &str) -> bool;

    /// 是否为 macOS 系统私有目录别名（`/etc` → `/private/etc` 等）。
    fn is_system_alias(&self, path: &Path) -> bool;
}
