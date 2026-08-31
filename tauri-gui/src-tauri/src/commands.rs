// commands.rs — Tauri IPC commands that wrap tridentd gRPC

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

// ─────────────────────────────────────────────────────────────────────────────
// Data types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceInfo {
    pub instance_id: String,
    pub adb_host: String,
    pub adb_port: u32,
    pub display_sock: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmConfig {
    pub vcpu_count: u32,
    pub memory_mib: u64,
    pub kernel_path: String,
    pub initrd_path: Option<String>,
    pub cmdline: String,
    pub sriov_vf: Option<String>,
    pub system_image: Option<String>,
    pub vendor_image: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppSettings {
    pub grpc_host: String,
    pub grpc_port: u16,
    pub kernel_path: String,
    pub system_image_path: String,
    pub vendor_image_path: String,
    pub theme: String,
    pub vcpu_default: u32,
    pub memory_default_mib: u64,
}

#[derive(Debug, Clone, Default)]
pub struct AppState {
    pub instances: Arc<Mutex<HashMap<String, InstanceInfo>>>,
    pub settings: Arc<Mutex<AppSettings>>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Commands
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn ping_daemon() -> Result<String, String> {
    Ok("pong".to_string())
}

#[tauri::command]
pub async fn launch_instance(
    config: VmConfig,
    state: tauri::State<'_, AppState>,
) -> Result<InstanceInfo, String> {
    // In a full implementation, this would call tridentd gRPC.
    // For now, we create a local instance entry.
    let id = format!("vm-{:08x}", rand::random::<u32>());
    
    let info = InstanceInfo {
        instance_id: id.clone(),
        adb_host: "127.0.0.1".to_string(),
        adb_port: 5555,
        display_sock: format!("/tmp/trident-{}-display.sock", id),
        state: "booting".to_string(),
    };

    state.instances.lock().await.insert(id, info.clone());
    Ok(info)
}

#[tauri::command]
pub async fn list_instances(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<InstanceInfo>, String> {
    let instances = state.instances.lock().await;
    Ok(instances.values().cloned().collect())
}

#[tauri::command]
pub async fn stop_instance(
    instance_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let mut instances = state.instances.lock().await;
    Ok(instances.remove(&instance_id).is_some())
}

#[tauri::command]
pub async fn get_instance_info(
    instance_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Option<InstanceInfo>, String> {
    let instances = state.instances.lock().await;
    Ok(instances.get(&instance_id).cloned())
}

#[tauri::command]
pub async fn fork_instance(
    instance_id: String,
    count: u32,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<InstanceInfo>, String> {
    let mut children = Vec::new();
    for i in 0..count {
        let child_id = format!("{}-fork-{}", instance_id, i);
        let info = InstanceInfo {
            instance_id: child_id,
            adb_host: "127.0.0.1".to_string(),
            adb_port: 5555,
            display_sock: String::new(),
            state: "booting".to_string(),
        };
        children.push(info);
    }
    Ok(children)
}

#[tauri::command]
pub async fn adb_shell_command(
    instance_id: String,
    command: String,
) -> Result<String, String> {
    // In a full implementation, this would connect to the ADB shell stream.
    tracing::info!("ADB shell [{}]: {}", instance_id, command);
    Ok(format!("{}: command executed\n", command))
}

#[tauri::command]
pub async fn check_updates() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "available": false,
        "version": env!("CARGO_PKG_VERSION"),
        "notes": ""
    }))
}

#[tauri::command]
pub async fn get_settings(
    state: tauri::State<'_, AppState>,
) -> Result<AppSettings, String> {
    Ok(state.settings.lock().await.clone())
}

#[tauri::command]
pub async fn save_settings(
    settings: AppSettings,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    *state.settings.lock().await = settings;
    Ok(())
}
