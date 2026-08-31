// streaming.rs — Tauri-managed streaming sessions

use crate::client::DaemonClient;
use crate::client::proto;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

#[derive(Debug)]
pub struct StreamingState {
    pub adb_sessions: Arc<Mutex<HashMap<String, tokio::sync::mpsc::Sender<Vec<u8>>>>>,
    pub display_sessions: Arc<Mutex<HashMap<String, tokio::sync::mpsc::Sender<()>>>>,
}

impl Default for StreamingState {
    fn default() -> Self {
        Self {
            adb_sessions: Arc::new(Mutex::new(HashMap::new())),
            display_sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[tauri::command]
pub async fn start_adb_shell(
    app: AppHandle,
    instance_id: String,
    state: State<'_, super::commands::AppState>,
    streaming: State<'_, StreamingState>,
) -> Result<(), String> {
    let mut daemon = state.daemon.lock().await;
    if daemon.is_none() {
        return Err("Not connected to daemon".to_string());
    }
    
    let (tx, rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    let mut resp_rx = daemon.as_mut().unwrap()
        .adb_shell(instance_id.clone(), rx)
        .await
        .map_err(|e| format!("Failed to start ADB shell: {}", e))?;

    // Store the sender so we can send commands later
    streaming.adb_sessions.lock().await.insert(instance_id.clone(), tx);

    // Spawn task to receive responses and emit events
    let app_clone = app.clone();
    let instance_id_clone = instance_id.clone();
    tokio::spawn(async move {
        while let Some(data) = resp_rx.recv().await {
            let text = String::from_utf8_lossy(&data).to_string();
            if app_clone.emit(&format!("adb_shell_{}", instance_id_clone), text).is_err() {
                break;
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn send_adb_command(
    instance_id: String,
    command: String,
    streaming: State<'_, StreamingState>,
) -> Result<(), String> {
    let sessions = streaming.adb_sessions.lock().await;
    if let Some(tx) = sessions.get(&instance_id) {
        tx.send(command.into_bytes())
            .await
            .map_err(|e| format!("Failed to send command: {}", e))?;
        Ok(())
    } else {
        Err("No active ADB shell session".to_string())
    }
}

#[tauri::command]
pub async fn start_display_stream(
    app: AppHandle,
    instance_id: String,
    state: State<'_, super::commands::AppState>,
    streaming: State<'_, StreamingState>,
) -> Result<(), String> {
    let mut daemon = state.daemon.lock().await;
    if daemon.is_none() {
        return Err("Not connected to daemon".to_string());
    }
    
    let (tx, _rx) = tokio::sync::mpsc::channel::<()>(16);
    let mut frame_rx = daemon.as_mut().unwrap()
        .stream_display(instance_id.clone())
        .await
        .map_err(|e| format!("Failed to start display stream: {}", e))?;

    // Store the session
    streaming.display_sessions.lock().await.insert(instance_id.clone(), tx);

    // Spawn task to receive frames and emit events
    let app_clone = app.clone();
    let instance_id_clone = instance_id.clone();
    tokio::spawn(async move {
        while let Some(frame) = frame_rx.recv().await {
            // Convert frame data to serializable format
            let frame_data = frame.data.iter().map(|&b| b as u32).collect::<Vec<u32>>();
            let payload = serde_json::json!({
                "data": frame_data,
                "pts_us": frame.pts_us,
                "key_frame": frame.key_frame,
                "width": frame.width,
                "height": frame.height,
            });
            if app_clone.emit(&format!("display_frame_{}", instance_id_clone), payload).is_err() {
                break;
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn close_adb_shell(
    instance_id: String,
    streaming: State<'_, StreamingState>,
) -> Result<(), String> {
    streaming.adb_sessions.lock().await.remove(&instance_id);
    Ok(())
}

#[tauri::command]
pub async fn close_display_stream(
    instance_id: String,
    streaming: State<'_, StreamingState>,
) -> Result<(), String> {
    streaming.display_sessions.lock().await.remove(&instance_id);
    Ok(())
}
