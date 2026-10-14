// MCTier 后端模块
// 本文件作为后端模块的入口点，用于组织和导出各个子模块
//
// # 平台化重构中的迁移说明
//
// 平台无关模块正分批迁往 `mctier-core`。**旧路径在这里重导出**，因此现有
// `crate::modules::X` 引用无需改动，迁移得以逐批验证而不是一次性大搬迁。
// 本文件是唯一需要跟着迁移改的地方；调用方与测试都不受影响。

// ============================================================
//  已迁往 mctier-core 的平台无关模块（重导出以保持旧路径可用）
// ============================================================

// 基础层
pub use mctier_core::{app_paths, error, logs};

// 网络与安全基础
pub use mctier_core::{hosts_security, http_cors, virtual_network};

// 身份与密钥
pub use mctier_core::{chat_auth, secret_store};

// 配置与寻址
pub use mctier_core::{config_manager, lobby_address};

// 服务层
pub use mctier_core::{
    chat_service, file_transfer, p2p_signaling, websocket_signaling,
};

// Minecraft 集成
pub use mctier_core::{mc_lan_bridge, minecraft_discovery};

// ============================================================
//  仍在本 crate 的模块
// ============================================================

// 资源管理模块
pub mod resource_manager;

// 网络服务模块
pub mod network_service;

// 大厅管理模块
pub mod lobby_manager;

// Hosts文件管理模块（Magic DNS）
pub mod helper_handshake;
pub mod hosts_manager;
#[cfg(unix)]
pub mod unix_hosts_helper;

// 语音服务模块
#[cfg(windows)]
pub mod privileged_helper;
// macOS：utun 必须由 root 创建，EasyTier 通过系统授权窗口提权运行
#[cfg(target_os = "macos")]
pub mod macos_privilege;
// macOS：常驻特权守护进程（一次授权、运行期复用，不再每次大厅弹密码）
#[cfg(target_os = "macos")]
pub mod macos_privileged_daemon;
pub mod voice_service;
#[cfg(windows)]
pub mod windows_paths;

// 应用核心模块
pub mod app_core;

// Tauri 命令接口模块
pub mod tauri_commands;

// EasyTier 高级配置命令模块
pub mod easytier_advanced_commands;

// Tauri 事件推送模块
pub mod tauri_events;

// 内置表情
pub mod builtin_emoji;

// 远程控制（输入注入）模块
pub mod remote_control;
pub mod speech_transcription;
pub mod media_permission;
pub mod native_capture;
pub mod native_microphone;
pub mod voice_ice;
pub mod quark_support;
pub mod quark_background;
#[cfg(windows)]
mod quark_scheduler;
#[cfg(all(test, windows))]
mod quark_webview_test;

// Linux 平台支撑（TUN 文件能力 / 网卡检测 / 防火墙 / XDG 自启动）
#[cfg(target_os = "linux")]
pub mod linux_platform;

// macOS 平台支撑（网卡检测 / LaunchAgent 自启动）
#[cfg(target_os = "macos")]
pub mod macos_platform;

// macOS 应用菜单栏（关于 / 设置 / 编辑 / 窗口 / 帮助）。文案组装可在非 macOS
// 上编译测试，真正装到 AppKit 的函数只在 macOS 生效。
pub mod macos_menu;

// macOS 防休眠断言（录制/通话期间阻止系统休眠，区别于 App Nap 豁免）。
#[cfg(target_os = "macos")]
pub mod macos_power;

pub mod screen_recording;
