//! MCTier macOS 原生平台策略实现（`mctier-platform-macos`）。
//!
//! 彻底剥离 Windows `WS_POPUP` 无边框透明窗的历史规避代码：
//! - 关闭按钮隐藏窗口时直接调用 `window.hide()`，**不**做前置 `unminimize()`，
//!   **不**篡改 `always_on_top`，**不**派生 300ms 延迟线程；
//! - 最小化走 macOS 原生黄色按钮缩小到 Dock；
//! - 点 Dock 图标（`RunEvent::Reopen`）或菜单栏图标时，若窗口处于最小化则先
//!   `unminimize()`，再 `show()` + `set_focus()` 立即唤回；
//! - 后台提示文案遵循 macOS 顶部菜单栏约定，不再出现「点击右下角托盘图标」。

use mctier_platform::{Autostart, CloseAction, Notifier, PathRules, WindowPolicy};
use std::path::Path;
use tauri::{AppHandle, WebviewWindow};

/// macOS 原生窗口生命周期策略。
#[derive(Debug, Default, Clone, Copy)]
pub struct MacOsWindowPolicy;

impl WindowPolicy for MacOsWindowPolicy {
    fn default_close_to_background(&self) -> bool {
        true
    }

    fn hide_to_background(&self, window: &WebviewWindow, source: &str) -> Result<bool, String> {
        let was_visible = window.is_visible().unwrap_or(false);
        let was_minimized = window.is_minimized().unwrap_or(false);
        if !was_visible && !was_minimized && source != "关闭按钮" {
            log::info!("{source}：MCTier 已处于菜单栏后台，跳过重复隐藏");
            return Ok(false);
        }
        // macOS 原生行为：直接 orderOut (hide)，不触碰 miniaturize 与 window level。
        window
            .hide()
            .map_err(|e| format!("{source} 隐藏主窗口到菜单栏失败: {e}"))?;
        log::info!("{source}：MCTier 已隐藏到 macOS 菜单栏");
        Ok(was_visible)
    }

    fn restore(&self, window: &WebviewWindow) -> Result<(), String> {
        if window.is_minimized().unwrap_or(false) {
            let _ = window.unminimize();
        }
        window
            .show()
            .map_err(|e| format!("显示 macOS 主窗口失败: {e}"))?;
        let _ = window.set_focus();
        Ok(())
    }
}

/// macOS 通知中心实现。
#[derive(Debug, Default, Clone, Copy)]
pub struct MacOsNotifier;

impl Notifier for MacOsNotifier {
    fn default_background_notice(&self) -> (&'static str, &'static str) {
        (
            "MCTier 正在后台运行",
            "MCTier 已隐藏到顶部菜单栏。点击菜单栏图标、Dock 图标或按 {shortcut} 可恢复窗口。",
        )
    }

    fn notify_running_in_background(&self, _app: &AppHandle, title: &str, body: &str) {
        let escape = |text: &str| text.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            escape(body),
            escape(title)
        );
        match std::process::Command::new("/usr/bin/osascript")
            .arg("-e")
            .arg(&script)
            .spawn()
        {
            Ok(_) => log::info!("已发送 macOS 后台运行通知"),
            Err(error) => log::warn!("发送 macOS 后台运行通知失败: {error}"),
        }
    }
}

/// macOS 路径与文件名规则。
#[derive(Debug, Default, Clone, Copy)]
pub struct MacOsPathRules;

impl PathRules for MacOsPathRules {
    fn is_reserved_device_name(&self, _name: &str) -> bool {
        // POSIX 文件系统无 CON/PRN/NUL 保留设备文件名限制（跨平台传输仍可按需显式校验）。
        false
    }

    fn is_system_alias(&self, path: &Path) -> bool {
        let raw = path.to_string_lossy();
        matches!(raw.as_ref(), "/etc" | "/var" | "/tmp")
            || raw.starts_with("/etc/")
            || raw.starts_with("/var/")
            || raw.starts_with("/tmp/")
    }
}

/// macOS LaunchAgent 自启占位适配器（实际写入逻辑位于 macos_platform::set_auto_start）。
#[derive(Debug, Default, Clone, Copy)]
pub struct MacOsAutostart;

impl Autostart for MacOsAutostart {
    fn set_enabled(&self, _enable: bool) -> Result<(), String> {
        Ok(())
    }

    fn is_enabled(&self) -> bool {
        false
    }
}

pub const WINDOW_POLICY: MacOsWindowPolicy = MacOsWindowPolicy;
pub const NOTIFIER: MacOsNotifier = MacOsNotifier;
pub const PATH_RULES: MacOsPathRules = MacOsPathRules;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_defaults_to_hiding_on_close() {
        assert!(WINDOW_POLICY.default_close_to_background());
        assert_eq!(
            WINDOW_POLICY.close_action(None),
            CloseAction::HideToBackground
        );
        assert_eq!(
            WINDOW_POLICY.close_action(Some(true)),
            CloseAction::HideToBackground
        );
        assert_eq!(WINDOW_POLICY.close_action(Some(false)), CloseAction::Quit);
    }

    #[test]
    fn macos_background_notice_mentions_menu_bar_not_bottom_right_tray() {
        let (title, body) = NOTIFIER.default_background_notice();
        assert!(!title.is_empty());
        assert!(body.contains("菜单栏"), "应提及顶部菜单栏: {body}");
        assert!(!body.contains("右下角"), "不得残留 Windows 右下角托盘文案: {body}");
    }

    #[test]
    fn macos_system_aliases_are_detected() {
        assert!(PATH_RULES.is_system_alias(Path::new("/etc/hosts")));
        assert!(PATH_RULES.is_system_alias(Path::new("/var/folders/x")));
        assert!(!PATH_RULES.is_system_alias(Path::new("/Users/example")));
    }
}
