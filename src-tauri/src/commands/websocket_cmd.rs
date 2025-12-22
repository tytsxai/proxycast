//! WebSocket 相关的 Tauri 命令

use crate::websocket::{WsConnection, WsStatsSnapshot};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// WebSocket 服务状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsServiceStatus {
    /// 是否启用
    pub enabled: bool,
    /// 活跃连接数
    pub active_connections: u64,
    /// 总连接数
    pub total_connections: u64,
    /// 总消息数
    pub total_messages: u64,
    /// 总错误数
    pub total_errors: u64,
}

/// WebSocket 连接详情
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsConnectionInfo {
    /// 连接 ID
    pub id: String,
    /// 连接时间
    pub connected_at: String,
    /// 客户端信息
    pub client_info: Option<String>,
    /// 请求计数
    pub request_count: u64,
}

impl From<WsConnection> for WsConnectionInfo {
    fn from(conn: WsConnection) -> Self {
        Self {
            id: conn.id,
            connected_at: conn.connected_at.to_rfc3339(),
            client_info: conn.client_info,
            request_count: conn.request_count,
        }
    }
}

/// WebSocket 状态封装（用于 Tauri State）
#[allow(dead_code)]
pub struct WsServiceState {
    pub enabled: Arc<RwLock<bool>>,
    pub stats: Arc<RwLock<WsStatsSnapshot>>,
    pub connections: Arc<RwLock<Vec<WsConnectionInfo>>>,
}

impl WsServiceState {
    pub fn new() -> Self {
        Self {
            enabled: Arc::new(RwLock::new(false)),
            stats: Arc::new(RwLock::new(WsStatsSnapshot {
                total_connections: 0,
                active_connections: 0,
                total_messages: 0,
                total_errors: 0,
            })),
            connections: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

impl Default for WsServiceState {
    fn default() -> Self {
        Self::new()
    }
}

/// 获取 WebSocket 服务状态
#[allow(dead_code)]
#[tauri::command]
pub async fn get_websocket_status(
    state: tauri::State<'_, WsServiceState>,
) -> Result<WsServiceStatus, String> {
    let enabled = *state.enabled.read().await;
    let stats = state.stats.read().await.clone();

    Ok(WsServiceStatus {
        enabled,
        active_connections: stats.active_connections,
        total_connections: stats.total_connections,
        total_messages: stats.total_messages,
        total_errors: stats.total_errors,
    })
}

/// 获取 WebSocket 连接列表
#[allow(dead_code)]
#[tauri::command]
pub async fn get_websocket_connections(
    state: tauri::State<'_, WsServiceState>,
) -> Result<Vec<WsConnectionInfo>, String> {
    let connections = state.connections.read().await.clone();
    Ok(connections)
}

/// 启用/禁用 WebSocket 服务
#[allow(dead_code)]
#[tauri::command]
pub async fn set_websocket_enabled(
    state: tauri::State<'_, WsServiceState>,
    enabled: bool,
) -> Result<(), String> {
    *state.enabled.write().await = enabled;
    Ok(())
}
