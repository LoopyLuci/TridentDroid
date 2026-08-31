// commands.rs — Tauri IPC commands that wrap tridentd gRPC

use crate::client::DaemonClient;
use crate::client::proto;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;
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
    pub daemon: Arc<Mutex<Option<DaemonClient>>>,
    pub settings: Arc<Mutex<AppSettings>>,
}

fn map_instance_info(info: &proto::InstanceInfo) -> InstanceInfo {
    InstanceInfo {
        instance_id: info.instance_id.clone(),
        adb_host: info.adb_host.clone(),
        adb_port: info.adb_port,
        display_sock: info.display_sock.clone(),
        state: format!("{:?}", proto::InstanceState::try_from(info.state).unwrap_or(proto::InstanceState::Unknown)),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Commands
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn ping_daemon(state: State<'_, AppState>) -> Result<String, String> {
    let mut daemon = state.daemon.lock().await;
    if daemon.is_none() {
        let settings = state.settings.lock().await;
        let addr = format!("http://{}:{}", settings.grpc_host, settings.grpc_port);
        match DaemonClient::connect(&addr).await {
            Ok(client) => {
                *daemon = Some(client);
            }
            Err(e) => return Err(format!("Cannot connect to daemon: {}", e)),
        }
    }
    let client = daemon.as_mut().unwrap();
    match client.ping().await {
        Ok(resp) => Ok(format!("pong (v{}, {} instances)", resp.version, resp.instance_count)),
        Err(e) => Err(format!("Ping failed: {}", e)),
    }
}

#[tauri::command]
pub async fn launch_instance(
    config: VmConfig,
    state: State<'_, AppState>,
) -> Result<InstanceInfo, String> {
    let mut daemon = state.daemon.lock().await;
    if daemon.is_none() {
        let settings = state.settings.lock().await;
        let addr = format!("http://{}:{}", settings.grpc_host, settings.grpc_port);
        match DaemonClient::connect(&addr).await {
            Ok(client) => {
                *daemon = Some(client);
            }
            Err(e) => return Err(format!("Cannot connect to daemon: {}", e)),
        }
    }
    let client = daemon.as_mut().unwrap();
    match client.launch_instance(
        config.kernel_path,
        config.system_image,
        config.vendor_image,
        config.cmdline,
        config.vcpu_count,
        config.memory_mib,
        config.sriov_vf,
    ).await {
        Ok(info) => {
            let mapped = map_instance_info(&info);
            Ok(mapped)
        }
        Err(e) => Err(format!("Launch failed: {}", e)),
    }
}

#[tauri::command]
pub async fn list_instances(
    _state: State<'_, AppState>,
) -> Result<Vec<InstanceInfo>, String> {
    // For now, we can't list instances via the gRPC proto since there's no List RPC.
    // Return an empty list - the frontend will use the launch response.
    Ok(vec![])
}

#[tauri::command]
pub async fn stop_instance(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let mut daemon = state.daemon.lock().await;
    if daemon.is_none() {
        let settings = state.settings.lock().await;
        let addr = format!("http://{}:{}", settings.grpc_host, settings.grpc_port);
        match DaemonClient::connect(&addr).await {
            Ok(client) => {
                *daemon = Some(client);
            }
            Err(e) => return Err(format!("Cannot connect to daemon: {}", e)),
        }
    }
    let client = daemon.as_mut().unwrap();
    match client.stop_instance(instance_id).await {
        Ok(success) => Ok(success),
        Err(e) => Err(format!("Stop failed: {}", e)),
    }
}

#[tauri::command]
pub async fn get_instance_info(
    _instance_id: String,
    _state: State<'_, AppState>,
) -> Result<Option<InstanceInfo>, String> {
    // No direct "get instance" RPC in the proto - return None for now
    Ok(None)
}

#[tauri::command]
pub async fn fork_instance(
    instance_id: String,
    count: u32,
    state: State<'_, AppState>,
) -> Result<Vec<InstanceInfo>, String> {
    let mut daemon = state.daemon.lock().await;
    if daemon.is_none() {
        let settings = state.settings.lock().await;
        let addr = format!("http://{}:{}", settings.grpc_host, settings.grpc_port);
        match DaemonClient::connect(&addr).await {
            Ok(client) => {
                *daemon = Some(client);
            }
            Err(e) => return Err(format!("Cannot connect to daemon: {}", e)),
        }
    }
    let client = daemon.as_mut().unwrap();
    match client.fork_instance(instance_id, count).await {
        Ok(children) => {
            let mapped = children.iter().map(|c| map_instance_info(c)).collect();
            Ok(mapped)
        }
        Err(e) => Err(format!("Fork failed: {}", e)),
    }
}

#[tauri::command]
pub async fn adb_shell_command(
    _instance_id: String,
    command: String,
) -> Result<String, String> {
    // ADB shell requires bidirectional streaming - not yet implemented in GUI client
    Ok(format!("{}: [streaming not yet available]\n", command))
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
    state: State<'_, AppState>,
) -> Result<AppSettings, String> {
    Ok(state.settings.lock().await.clone())
}

#[tauri::command]
pub async fn save_settings(
    settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<(), String> {
    *state.settings.lock().await = settings;
    Ok(())
}
