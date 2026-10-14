//! MCTier 平台无关核心。
//!
//! 这个 crate 承载与操作系统无关的业务逻辑。它的**不变量**是：不依赖也不引用任何
//! 平台 API（windows / objc2 / webkit2gtk …），平台差异一律通过 `mctier-platform`
//! 定义的 trait、由各平台实现 crate 注入。CI 会对本目录做 grep 强制检查。
//!
//! 迁移目前是分批进行的（见仓库根的计划）：先从**出度为 0 的叶子模块**开始，
//! 它们在旧位置（`src-tauri/src/modules/`）通过重导出保持原有路径可用，
//! 因此调用方无需改动，可以逐批验证而不产生大规模破坏。

pub mod app_paths;
pub mod chat_auth;
pub mod error;
pub mod http_cors;
pub mod logs;
