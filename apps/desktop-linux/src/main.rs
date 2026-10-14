//! MCTier Linux 独立应用壳入口。
//!
//! 本 crate 在编译期仅依赖 `mctier-core` + `mctier-platform` + `mctier-platform-linux`。

fn main() {
    let _ = &mctier_platform_linux::WINDOW_POLICY;
    let _ = &mctier_platform_linux::NOTIFIER;
    let _ = &mctier_platform_linux::PATH_RULES;
    mctier_lib::run()
}
