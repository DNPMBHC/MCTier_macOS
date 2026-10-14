//! MCTier macOS 独立应用壳入口。
//!
//! 本 crate 在编译期仅依赖 `mctier-core` + `mctier-platform` + `mctier-platform-macos`，
//! 从依赖图上杜绝直接引入 `windows` / `wasapi` / `webview2-com` / `webkit2gtk`。

fn main() {
    let _ = &mctier_platform_macos::WINDOW_POLICY;
    let _ = &mctier_platform_macos::NOTIFIER;
    let _ = &mctier_platform_macos::PATH_RULES;
    mctier_lib::run()
}
