//! MCTier 平台无关核心。
//!
//! 这个 crate 承载与操作系统无关的业务逻辑。它的**不变量**是：不依赖也不引用任何
//! 平台 API（windows / objc2 / webkit2gtk …），平台差异一律通过 `mctier-platform`
//! 定义的 trait、由各平台实现 crate 注入。CI 会对本目录做 grep 强制检查。
//!
//! # 布局约定
//!
//! 所有模块都是本 crate 的**顶层模块**（扁平，不嵌套 `modules::`）。这个布局让迁移
//! 可以增量进行：模块之间原本写的是 `super::other`，搬到本 crate 后 `super::` 恰好
//! 指向 crate 根，因此**引用原样有效**，不必逐个改写。旧位置
//! （`src-tauri/src/modules/`）通过重导出保持原有路径可用，调用方零改动。
//!
//! # 迁移状态
//!
//! 分批迁移，每批都以「出度为 0 或依赖闭合」为准，逐批验证：
//! - 第 1 批：error / app_paths / logs / http_cors / chat_auth
//! - 第 2 批：virtual_network / hosts_security / secret_store / config_manager /
//!   lobby_address / chat_service / file_transfer / p2p_signaling /
//!   websocket_signaling / minecraft_discovery / mc_lan_bridge

// —— 基础层 ——
pub mod app_paths;
pub mod error;
pub mod logs;

// —— 网络与安全基础 ——
pub mod http_cors;
pub mod virtual_network;
pub mod hosts_security;

// —— 身份与密钥 ——
pub mod chat_auth;
pub mod secret_store;

// —— 配置与寻址 ——
pub mod config_manager;
pub mod lobby_address;

// —— 服务层 ——
pub mod chat_service;
pub mod file_transfer;
pub mod p2p_signaling;
pub mod websocket_signaling;

// —— Minecraft 集成 ——
pub mod minecraft_discovery;
pub mod mc_lan_bridge;
