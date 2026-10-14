// Run from workspace root or src-tauri: cargo test --test native_security
#[path = "../src/modules/helper_handshake.rs"]
mod helper_handshake;
#[path = "../../crates/mctier-core/src/hosts_security.rs"]
mod hosts_security;
#[path = "../../crates/mctier-core/src/virtual_network.rs"]
mod virtual_network;
