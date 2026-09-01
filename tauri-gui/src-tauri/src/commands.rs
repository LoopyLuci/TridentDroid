// commands.rs — Tauri IPC commands that wrap tridentd gRPC

use crate::client::{ConnectionConfig, DaemonClient};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub snapshot_id: String,
    pub size_bytes: u64,
    pub duration_ms: u64,
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
    pub use_tls: bool,
    pub ca_cert_path: String,
    pub client_cert_path: String,
    pub client_key_path: String,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub daemon: Arc<Mutex<Option<DaemonClient>>>,
    pub settings: Arc<Mutex<AppSettings>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            daemon: Arc::new(Mutex::new(None)),
            settings: Arc::new(Mutex::new(load_settings())),
        }
    }
}

fn settings_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("TridentDroid").join("settings.json"))
}

fn load_settings() -> AppSettings {
    settings_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn persist_settings(settings: &AppSettings) -> Result<(), String> {
    let path = settings_path().ok_or_else(|| "Could not determine config directory".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
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

async fn get_client<'a>(state: &'a State<'a, AppState>) -> Result<tokio::sync::MutexGuard<'a, Option<DaemonClient>>, String> {
    let mut daemon = state.daemon.lock().await;
    if daemon.is_none() {
        let settings = state.settings.lock().await;
        let config = ConnectionConfig {
            host: settings.grpc_host.clone(),
            port: settings.grpc_port,
            use_tls: settings.use_tls,
            ca_cert_path: if settings.ca_cert_path.is_empty() { None } else { Some(settings.ca_cert_path.clone()) },
            client_cert_path: if settings.client_cert_path.is_empty() { None } else { Some(settings.client_cert_path.clone()) },
            client_key_path: if settings.client_key_path.is_empty() { None } else { Some(settings.client_key_path.clone()) },
        };
        match DaemonClient::connect_with_config(&config).await {
            Ok(client) => {
                *daemon = Some(client);
            }
            Err(e) => return Err(format!("Cannot connect to daemon: {}", e)),
        }
    }
    Ok(daemon)
}

// ─────────────────────────────────────────────────────────────────────────────
// Commands
// ─────────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn ping_daemon(state: State<'_, AppState>) -> Result<String, String> {
    let mut daemon = get_client(&state).await?;
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
    let mut daemon = get_client(&state).await?;
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
    state: State<'_, AppState>,
) -> Result<Vec<InstanceInfo>, String> {
    let mut daemon = get_client(&state).await?;
    let client = daemon.as_mut().unwrap();
    match client.list_instances().await {
        Ok(instances) => {
            let mapped = instances.iter().map(|i| map_instance_info(i)).collect();
            Ok(mapped)
        }
        Err(e) => Err(format!("List failed: {}", e)),
    }
}

#[tauri::command]
pub async fn stop_instance(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let mut daemon = get_client(&state).await?;
    let client = daemon.as_mut().unwrap();
    match client.stop_instance(instance_id).await {
        Ok(success) => Ok(success),
        Err(e) => Err(format!("Stop failed: {}", e)),
    }
}

#[tauri::command]
pub async fn get_instance_info(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<Option<InstanceInfo>, String> {
    let mut daemon = get_client(&state).await?;
    let client = daemon.as_mut().unwrap();
    match client.get_instance(instance_id).await {
        Ok(Some(info)) => Ok(Some(map_instance_info(&info))),
        Ok(None) => Ok(None),
        Err(e) => Err(format!("Get instance failed: {}", e)),
    }
}

#[tauri::command]
pub async fn fork_instance(
    instance_id: String,
    count: u32,
    state: State<'_, AppState>,
) -> Result<Vec<InstanceInfo>, String> {
    let mut daemon = get_client(&state).await?;
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
pub async fn create_snapshot(
    instance_id: String,
    snapshot_id: Option<String>,
    include_disk: bool,
    state: State<'_, AppState>,
) -> Result<SnapshotInfo, String> {
    let mut daemon = get_client(&state).await?;
    let client = daemon.as_mut().unwrap();
    match client.snapshot(instance_id, snapshot_id, include_disk).await {
        Ok(resp) => Ok(SnapshotInfo {
            snapshot_id: resp.snapshot_id,
            size_bytes: resp.size_bytes,
            duration_ms: resp.duration_ms,
        }),
        Err(e) => Err(format!("Snapshot failed: {}", e)),
    }
}

#[tauri::command]
pub async fn restore_snapshot(
    snapshot_id: String,
    vcpu_count: Option<u32>,
    memory_mib: Option<u64>,
    state: State<'_, AppState>,
) -> Result<InstanceInfo, String> {
    let mut daemon = get_client(&state).await?;
    let client = daemon.as_mut().unwrap();
    match client.restore(snapshot_id, vcpu_count, memory_mib).await {
        Ok(info) => Ok(map_instance_info(&info)),
        Err(e) => Err(format!("Restore failed: {}", e)),
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
    persist_settings(&settings)?;
    *state.settings.lock().await = settings;
    Ok(())
}
