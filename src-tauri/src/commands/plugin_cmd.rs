//! 插件系统相关命令

use crate::plugin::{PluginConfig, PluginInfo, PluginManager};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// 插件管理器状态
pub struct PluginManagerState(pub Arc<RwLock<PluginManager>>);

/// 插件状态响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginServiceStatus {
    pub enabled: bool,
    pub plugin_count: usize,
    pub plugins_dir: String,
}

/// 插件配置请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfigRequest {
    pub enabled: bool,
    pub timeout_ms: u64,
    pub settings: serde_json::Value,
}

/// 获取插件服务状态
#[tauri::command]
pub async fn get_plugin_status(
    state: tauri::State<'_, PluginManagerState>,
) -> Result<PluginServiceStatus, String> {
    let manager = state.0.read().await;
    Ok(PluginServiceStatus {
        enabled: true,
        plugin_count: manager.count(),
        plugins_dir: manager.plugins_dir().to_string_lossy().to_string(),
    })
}

/// 获取所有插件列表
#[tauri::command]
pub async fn get_plugins(
    state: tauri::State<'_, PluginManagerState>,
) -> Result<Vec<PluginInfo>, String> {
    let manager = state.0.read().await;
    Ok(manager.list().await)
}

/// 获取单个插件信息
#[tauri::command]
pub async fn get_plugin_info(
    state: tauri::State<'_, PluginManagerState>,
    name: String,
) -> Result<Option<PluginInfo>, String> {
    let manager = state.0.read().await;
    Ok(manager.get_info(&name).await)
}

/// 启用插件
#[tauri::command]
pub async fn enable_plugin(
    state: tauri::State<'_, PluginManagerState>,
    name: String,
) -> Result<(), String> {
    let manager = state.0.read().await;
    manager.enable(&name).await.map_err(|e| e.to_string())
}

/// 禁用插件
#[tauri::command]
pub async fn disable_plugin(
    state: tauri::State<'_, PluginManagerState>,
    name: String,
) -> Result<(), String> {
    let manager = state.0.read().await;
    manager.disable(&name).await.map_err(|e| e.to_string())
}

/// 更新插件配置
#[tauri::command]
pub async fn update_plugin_config(
    state: tauri::State<'_, PluginManagerState>,
    name: String,
    config: PluginConfigRequest,
) -> Result<(), String> {
    let manager = state.0.read().await;
    let plugin_config = PluginConfig {
        enabled: config.enabled,
        timeout_ms: config.timeout_ms,
        settings: config.settings,
    };
    manager
        .update_config(&name, plugin_config)
        .await
        .map_err(|e| e.to_string())
}

/// 获取插件配置
#[tauri::command]
pub async fn get_plugin_config(
    state: tauri::State<'_, PluginManagerState>,
    name: String,
) -> Result<Option<PluginConfig>, String> {
    let manager = state.0.read().await;
    Ok(manager.get_config(&name))
}

/// 重新加载所有插件
#[tauri::command]
pub async fn reload_plugins(
    state: tauri::State<'_, PluginManagerState>,
) -> Result<Vec<String>, String> {
    let manager = state.0.read().await;
    manager.load_all().await.map_err(|e| e.to_string())
}

/// 卸载插件
#[tauri::command]
pub async fn unload_plugin(
    state: tauri::State<'_, PluginManagerState>,
    name: String,
) -> Result<(), String> {
    let manager = state.0.read().await;
    manager.unload(&name).await.map_err(|e| e.to_string())
}

/// 获取插件目录路径
#[tauri::command]
pub async fn get_plugins_dir(
    state: tauri::State<'_, PluginManagerState>,
) -> Result<String, String> {
    let manager = state.0.read().await;
    Ok(manager.plugins_dir().to_string_lossy().to_string())
}
