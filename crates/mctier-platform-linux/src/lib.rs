//! MCTier Linux 平台策略实现（`mctier-platform-linux`）。

use mctier_platform::{CloseAction, Notifier, PathRules, WindowPolicy};
use std::path::Path;
use tauri::{AppHandle, WebviewWindow};

/// Linux 窗口生命周期策略。
#[derive(Debug, Default, Clone, Copy)]
pub struct LinuxWindowPolicy;

impl WindowPolicy for LinuxWindowPolicy {
    fn default_close_to_background(&self) -> bool {
        false
    }

    fn hide_to_background(&self, window: &WebviewWindow, source: &str) -> Result<bool, String> {
        let was_visible = window.is_visible().unwrap_or(false);
        if !was_visible && source != "关闭按钮" {
            log::info!("{source}：MCTier 已处于后台，跳过重复隐藏");
            return Ok(false);
        }
        window
            .hide()
            .map_err(|e| format!("{source} 隐藏主窗口失败: {e}"))?;
        Ok(true)
    }

    fn restore(&self, window: &WebviewWindow) -> Result<(), String> {
        let _ = window.unminimize();
        window
            .show()
            .map_err(|e| format!("显示 Linux 主窗口失败: {e}"))?;
        let _ = window.set_focus();
        Ok(())
    }
}

/// Linux 通知实现（桌面环境行为不一，记录日志）。
#[derive(Debug, Default, Clone, Copy)]
pub struct LinuxNotifier;

impl Notifier for LinuxNotifier {
    fn default_background_notice(&self) -> (&'static str, &'static str) {
        (
            "MCTier 正在后台运行",
            "MCTier 已隐藏到系统托盘。点击托盘图标或按 {shortcut} 可恢复窗口。",
        )
    }

    fn notify_running_in_background(&self, _app: &AppHandle, title: &str, body: &str) {
        log::info!("托盘后台运行提示（Linux）: {title} - {body}");
    }
}

/// Linux 路径规则。
#[derive(Debug, Default, Clone, Copy)]
pub struct LinuxPathRules;

impl PathRules for LinuxPathRules {
    fn is_reserved_device_name(&self, _name: &str) -> bool {
        false
    }

    fn is_system_alias(&self, _path: &Path) -> bool {
        false
    }
}

pub const WINDOW_POLICY: LinuxWindowPolicy = LinuxWindowPolicy;
pub const NOTIFIER: LinuxNotifier = LinuxNotifier;
pub const PATH_RULES: LinuxPathRules = LinuxPathRules;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_defaults_to_quitting_on_close() {
        assert!(!WINDOW_POLICY.default_close_to_background());
        assert_eq!(WINDOW_POLICY.close_action(None), CloseAction::Quit);
    }
}
