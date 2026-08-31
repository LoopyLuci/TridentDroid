pub mod vmm;
pub mod gpu;
pub mod server;
pub mod platform;
pub mod security;

// Re-export the proto-generated types for Tauri GUI client
pub mod tridentd {
    tonic::include_proto!("tridentd");
}
