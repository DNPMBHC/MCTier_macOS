//! MCTier Windows 平台策略实现（`mctier-platform-windows`）。
//!
//! 集中承载无边框透明窗（`WS_POPUP`）在 Windows 任务栏与 `Win+D` 下的全部规避逻辑，
//! 确保这些 Win32 专有操作永不泄漏到 macOS 或 Linux。

use mctier_platform::{CloseAction, Notifier, PathRules, WindowPolicy};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, WebviewWindow};

static RESTORE_ALWAYS_ON_TOP_AFTER_TRAY: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
unsafe extern "system" fn window_subclass_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
    _id: usize,
    _data: usize,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::LRESULT;
    use windows::Win32::UI::Shell::DefSubclassProc;
    use windows::Win32::UI::WindowsAndMessaging::{
        ShowWindow, SC_MINIMIZE, SW_HIDE, WM_SYSCOMMAND,
    };

    if msg == WM_SYSCOMMAND && (wparam.0 & 0xFFF0) == SC_MINIMIZE as usize {
        let _ = ShowWindow(hwnd, SW_HIDE);
        return LRESULT(0);
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

#[cfg(windows)]
fn release_windows_foreground_after_hide(window: &WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetShellWindow, GetWindowThreadProcessId, SetForegroundWindow,
        SetWindowPos, HWND_NOTOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };

    if let Ok(hwnd) = window.hwnd() {
        let hwnd = HWND(hwnd.0 as *mut _);
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                HWND_NOTOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );

            let foreground = GetForegroundWindow();
            let mut foreground_process_id = 0;
            GetWindowThreadProcessId(foreground, Some(&mut foreground_process_id));
            if foreground_process_id == std::process::id() {
                let shell = GetShellWindow();
                if !shell.0.is_null() {
                    let _ = SetForegroundWindow(shell);
                }
            }
        }
    }
}

/// Windows 窗口生命周期策略。
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsWindowPolicy;

impl WindowPolicy for WindowsWindowPolicy {
    fn default_close_to_background(&self) -> bool {
        false
    }

    fn install_window_hooks(&self, window: &WebviewWindow) {
        #[cfg(windows)]
        {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::Shell::SetWindowSubclass;
            if let Ok(hwnd) = window.hwnd() {
                let hwnd = HWND(hwnd.0 as *mut _);
                unsafe {
                    let _ = SetWindowSubclass(hwnd, Some(window_subclass_proc), 1, 0);
                }
            }
        }
        #[cfg(not(windows))]
        let _ = window;
    }

    fn hide_to_background(&self, window: &WebviewWindow, source: &str) -> Result<bool, String> {
        let was_visible = window.is_visible().unwrap_or(false);
        if !was_visible && source != "关闭按钮" {
            log::info!("{source}：MCTier 已经处于系统托盘，不重复发送后台运行通知");
            return Ok(false);
        }

        // Clear the minimized state before hiding so restore does not inherit a
        // stale taskbar/Win+D state from the previous transition.
        let _ = window.unminimize();
        let was_always_on_top = window.is_always_on_top().unwrap_or(false);
        RESTORE_ALWAYS_ON_TOP_AFTER_TRAY.store(was_always_on_top, Ordering::Release);
        if was_always_on_top {
            let _ = window.set_always_on_top(false);
        }
        window
            .hide()
            .map_err(|e| format!("{source} 隐藏主窗口到托盘失败: {e}"))?;
        #[cfg(windows)]
        release_windows_foreground_after_hide(window);
        Ok(true)
    }

    fn restore(&self, window: &WebviewWindow) -> Result<(), String> {
        let _ = window.unminimize();
        let _ = window.show();

        #[cfg(windows)]
        if let Ok(hwnd) = window.hwnd() {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::WindowsAndMessaging::{
                BringWindowToTop, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
            };
            let h = HWND(hwnd.0 as *mut _);
            unsafe {
                let _ = ShowWindow(h, SW_SHOW);
                if IsIconic(h).as_bool() {
                    let _ = ShowWindow(h, SW_RESTORE);
                }
                let _ = BringWindowToTop(h);
                let _ = SetForegroundWindow(h);
            }
        }

        let _ = window.set_focus();
        if RESTORE_ALWAYS_ON_TOP_AFTER_TRAY.swap(false, Ordering::AcqRel) {
            let _ = window.set_always_on_top(true);
        }
        Ok(())
    }
}

/// Windows 托盘通知文案策略（底层 Shell_NotifyIconW 气泡调用由壳层承接）。
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsNotifier;

impl Notifier for WindowsNotifier {
    fn default_background_notice(&self) -> (&'static str, &'static str) {
        (
            "MCTier 正在后台运行",
            "MCTier 已最小化到系统托盘。点击右下角托盘图标或按 {shortcut} 可恢复窗口。",
        )
    }

    fn notify_running_in_background(&self, _app: &AppHandle, title: &str, body: &str) {
        log::info!("Windows 托盘后台运行提示: {title} - {body}");
    }
}

/// Windows 路径与保留设备文件名规则。
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsPathRules;

impl PathRules for WindowsPathRules {
    fn is_reserved_device_name(&self, name: &str) -> bool {
        let stem = name
            .split('.')
            .next()
            .unwrap_or("")
            .trim_end_matches([' ', '.']);
        matches!(
            stem.to_ascii_uppercase().as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        )
    }

    fn is_system_alias(&self, _path: &Path) -> bool {
        false
    }
}

pub const WINDOW_POLICY: WindowsWindowPolicy = WindowsWindowPolicy;
pub const NOTIFIER: WindowsNotifier = WindowsNotifier;
pub const PATH_RULES: WindowsPathRules = WindowsPathRules;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_defaults_to_quitting_on_close() {
        assert!(!WINDOW_POLICY.default_close_to_background());
        assert_eq!(WINDOW_POLICY.close_action(None), CloseAction::Quit);
        assert_eq!(
            WINDOW_POLICY.close_action(Some(true)),
            CloseAction::HideToBackground
        );
    }

    #[test]
    fn windows_reserved_names_are_detected() {
        assert!(PATH_RULES.is_reserved_device_name("CON"));
        assert!(PATH_RULES.is_reserved_device_name("nul.txt"));
        assert!(!PATH_RULES.is_reserved_device_name("normal.txt"));
    }
}
