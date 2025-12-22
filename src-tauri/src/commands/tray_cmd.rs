//! 托盘相关命令
//!
//! 提供托盘状态同步和更新的 Tauri 命令
//!
//! # Requirements
//! - 7.1: API 服务器状态变化时在 1 秒内更新托盘图标
//! - 7.2: 凭证健康状态变化时在 1 秒内更新托盘图标
//! - 7.3: 托盘菜单打开时获取并显示最新信息

use crate::tray::{TrayIconStatus, TrayStateSnapshot};
use crate::TrayManagerState;
use tauri::State;
use tracing::{debug, info};

/// 同步托盘状态
///
/// 从前端或其他模块调用，更新托盘的完整状态
///
/// # Requirements
/// - 7.1: API 服务器状态变化时更新托盘图标
/// - 7.2: 凭证健康状态变化时更新托盘图标
#[tauri::command]
pub async fn sync_tray_state(
    tray_state: State<'_, TrayManagerState<tauri::Wry>>,
    server_running: bool,
    server_address: String,
    available_credentials: usize,
    total_credentials: usize,
    today_requests: u64,
    auto_start_enabled: bool,
) -> Result<(), String> {
    let tray_guard = tray_state.0.read().await;
    let tray_manager = tray_guard
        .as_ref()
        .ok_or_else(|| "托盘管理器未初始化".to_string())?;

    // 计算图标状态
    let icon_status = if !server_running {
        TrayIconStatus::Stopped
    } else if available_credentials == 0 && total_credentials > 0 {
        TrayIconStatus::Error
    } else if available_credentials < total_credentials {
        TrayIconStatus::Warning
    } else {
        TrayIconStatus::Running
    };

    let snapshot = TrayStateSnapshot {
        icon_status,
        server_running,
        server_address,
        available_credentials,
        total_credentials,
        today_requests,
        auto_start_enabled,
    };

    tray_manager
        .update_state(snapshot)
        .await
        .map_err(|e| e.to_string())?;

    debug!(
        "托盘状态已同步: server_running={}, icon_status={:?}",
        server_running, icon_status
    );

    Ok(())
}

/// 更新托盘服务器状态
///
/// 仅更新服务器运行状态
///
/// # Requirements
/// - 7.1: API 服务器状态变化时在 1 秒内更新托盘图标
#[tauri::command]
pub async fn update_tray_server_status(
    tray_state: State<'_, TrayManagerState<tauri::Wry>>,
    server_running: bool,
    server_host: String,
    server_port: u16,
) -> Result<(), String> {
    let tray_guard = tray_state.0.read().await;
    let tray_manager = tray_guard
        .as_ref()
        .ok_or_else(|| "托盘管理器未初始化".to_string())?;

    // 获取当前状态
    let mut current_state = tray_manager.get_state().await;

    // 更新服务器相关字段
    current_state.server_running = server_running;
    current_state.server_address = if server_running {
        format!("{}:{}", server_host, server_port)
    } else {
        String::new()
    };

    // 重新计算图标状态
    if !server_running {
        current_state.icon_status = TrayIconStatus::Stopped;
    } else if current_state.icon_status == TrayIconStatus::Stopped {
        current_state.icon_status = TrayIconStatus::Running;
    }

    tray_manager
        .update_state(current_state)
        .await
        .map_err(|e| e.to_string())?;

    info!(
        "托盘服务器状态已更新: running={}, address={}:{}",
        server_running, server_host, server_port
    );

    Ok(())
}

/// 更新托盘凭证状态
///
/// 仅更新凭证健康状态
///
/// # Requirements
/// - 7.2: 凭证健康状态变化时在 1 秒内更新托盘图标
#[tauri::command]
pub async fn update_tray_credential_status(
    tray_state: State<'_, TrayManagerState<tauri::Wry>>,
    available_credentials: usize,
    total_credentials: usize,
    has_warning: bool,
) -> Result<(), String> {
    let tray_guard = tray_state.0.read().await;
    let tray_manager = tray_guard
        .as_ref()
        .ok_or_else(|| "托盘管理器未初始化".to_string())?;

    // 获取当前状态
    let mut current_state = tray_manager.get_state().await;

    // 更新凭证相关字段
    current_state.available_credentials = available_credentials;
    current_state.total_credentials = total_credentials;

    // 重新计算图标状态
    if current_state.server_running {
        if available_credentials == 0 && total_credentials > 0 {
            current_state.icon_status = TrayIconStatus::Error;
        } else if has_warning || available_credentials < total_credentials {
            current_state.icon_status = TrayIconStatus::Warning;
        } else {
            current_state.icon_status = TrayIconStatus::Running;
        }
    }

    tray_manager
        .update_state(current_state)
        .await
        .map_err(|e| e.to_string())?;

    info!(
        "托盘凭证状态已更新: available={}/{}, has_warning={}",
        available_credentials, total_credentials, has_warning
    );

    Ok(())
}

/// 获取托盘当前状态
///
/// 返回托盘的当前状态快照
#[tauri::command]
pub async fn get_tray_state(
    tray_state: State<'_, TrayManagerState<tauri::Wry>>,
) -> Result<TrayStateSnapshot, String> {
    let tray_guard = tray_state.0.read().await;
    let tray_manager = tray_guard
        .as_ref()
        .ok_or_else(|| "托盘管理器未初始化".to_string())?;

    Ok(tray_manager.get_state().await)
}

/// 刷新托盘菜单
///
/// 强制刷新托盘菜单内容
///
/// # Requirements
/// - 7.3: 托盘菜单打开时获取并显示最新信息
#[tauri::command]
pub async fn refresh_tray_menu(
    tray_state: State<'_, TrayManagerState<tauri::Wry>>,
) -> Result<(), String> {
    let tray_guard = tray_state.0.read().await;
    let tray_manager = tray_guard
        .as_ref()
        .ok_or_else(|| "托盘管理器未初始化".to_string())?;

    tray_manager
        .refresh_menu()
        .await
        .map_err(|e| e.to_string())?;

    debug!("托盘菜单已刷新");

    Ok(())
}

/// 刷新托盘菜单并更新统计数据
///
/// 在菜单打开时调用，获取最新的统计数据并刷新菜单
///
/// # Requirements
/// - 7.3: 托盘菜单打开时获取并显示最新信息
#[tauri::command]
pub async fn refresh_tray_with_stats(
    tray_state: State<'_, TrayManagerState<tauri::Wry>>,
    server_running: bool,
    server_address: String,
    available_credentials: usize,
    total_credentials: usize,
    today_requests: u64,
    auto_start_enabled: bool,
) -> Result<(), String> {
    let tray_guard = tray_state.0.read().await;
    let tray_manager = tray_guard
        .as_ref()
        .ok_or_else(|| "托盘管理器未初始化".to_string())?;

    // 计算图标状态
    let icon_status = if !server_running {
        TrayIconStatus::Stopped
    } else if available_credentials == 0 && total_credentials > 0 {
        TrayIconStatus::Error
    } else if available_credentials < total_credentials {
        TrayIconStatus::Warning
    } else {
        TrayIconStatus::Running
    };

    let snapshot = TrayStateSnapshot {
        icon_status,
        server_running,
        server_address,
        available_credentials,
        total_credentials,
        today_requests,
        auto_start_enabled,
    };

    // 更新状态并刷新菜单
    tray_manager
        .update_state(snapshot)
        .await
        .map_err(|e| e.to_string())?;

    debug!(
        "托盘菜单已刷新: server_running={}, requests={}, credentials={}/{}",
        server_running, today_requests, available_credentials, total_credentials
    );

    Ok(())
}
