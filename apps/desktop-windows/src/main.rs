//! MCTier Windows 独立应用壳入口。
//!
//! 本 crate 在编译期仅依赖 `mctier-core` + `mctier-platform` + `mctier-platform-windows`。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let _ = &mctier_platform_windows::WINDOW_POLICY;
    let _ = &mctier_platform_windows::NOTIFIER;
    let _ = &mctier_platform_windows::PATH_RULES;
    mctier_lib::run()
}
