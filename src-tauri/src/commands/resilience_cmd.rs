//! 容错配置相关 Tauri 命令

use crate::resilience::{FailoverConfig, RetryConfig};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// 容错配置状态
pub struct ResilienceConfigState {
    pub retry_config: Arc<RwLock<RetryConfig>>,
    pub failover_config: Arc<RwLock<FailoverConfig>>,
    pub switch_log: Arc<RwLock<Vec<SwitchLogEntry>>>,
}

impl Default for ResilienceConfigState {
    fn default() -> Self {
        Self {
            retry_config: Arc::new(RwLock::new(RetryConfig::default())),
            failover_config: Arc::new(RwLock::new(FailoverConfig::default())),
            switch_log: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

/// 切换日志条目（用于前端显示）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchLogEntry {
    pub from_provider: String,
    pub to_provider: String,
    pub failure_type: String,
    pub timestamp: String,
}

/// 重试配置 DTO（用于前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfigDto {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub retryable_codes: Vec<u16>,
}

impl From<RetryConfig> for RetryConfigDto {
    fn from(config: RetryConfig) -> Self {
        Self {
            max_retries: config.max_retries,
            base_delay_ms: config.base_delay_ms,
            max_delay_ms: config.max_delay_ms,
            retryable_codes: config.retryable_codes,
        }
    }
}

impl From<RetryConfigDto> for RetryConfig {
    fn from(dto: RetryConfigDto) -> Self {
        Self {
            max_retries: dto.max_retries,
            base_delay_ms: dto.base_delay_ms,
            max_delay_ms: dto.max_delay_ms,
            retryable_codes: dto.retryable_codes,
        }
    }
}

/// 故障转移配置 DTO（用于前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverConfigDto {
    pub auto_switch: bool,
    pub switch_on_quota: bool,
}

impl From<FailoverConfig> for FailoverConfigDto {
    fn from(config: FailoverConfig) -> Self {
        Self {
            auto_switch: config.auto_switch,
            switch_on_quota: config.switch_on_quota,
        }
    }
}

impl From<FailoverConfigDto> for FailoverConfig {
    fn from(dto: FailoverConfigDto) -> Self {
        Self {
            auto_switch: dto.auto_switch,
            switch_on_quota: dto.switch_on_quota,
        }
    }
}

/// 获取重试配置
#[tauri::command]
pub async fn get_retry_config(
    state: tauri::State<'_, ResilienceConfigState>,
) -> Result<RetryConfigDto, String> {
    let config = state.retry_config.read().await;
    Ok(RetryConfigDto::from(config.clone()))
}

/// 更新重试配置
#[tauri::command]
pub async fn update_retry_config(
    state: tauri::State<'_, ResilienceConfigState>,
    config: RetryConfigDto,
) -> Result<(), String> {
    // 验证配置
    if config.max_retries > 10 {
        return Err("最大重试次数不能超过 10".to_string());
    }
    if config.base_delay_ms < 100 {
        return Err("基础延迟不能小于 100ms".to_string());
    }
    if config.max_delay_ms < config.base_delay_ms {
        return Err("最大延迟不能小于基础延迟".to_string());
    }
    if config.max_delay_ms > 120000 {
        return Err("最大延迟不能超过 120 秒".to_string());
    }

    let mut retry_config = state.retry_config.write().await;
    *retry_config = RetryConfig::from(config);
    Ok(())
}

/// 获取故障转移配置
#[tauri::command]
pub async fn get_failover_config(
    state: tauri::State<'_, ResilienceConfigState>,
) -> Result<FailoverConfigDto, String> {
    let config = state.failover_config.read().await;
    Ok(FailoverConfigDto::from(config.clone()))
}

/// 更新故障转移配置
#[tauri::command]
pub async fn update_failover_config(
    state: tauri::State<'_, ResilienceConfigState>,
    config: FailoverConfigDto,
) -> Result<(), String> {
    let mut failover_config = state.failover_config.write().await;
    *failover_config = FailoverConfig::from(config);
    Ok(())
}

/// 获取切换日志
#[tauri::command]
pub async fn get_switch_log(
    state: tauri::State<'_, ResilienceConfigState>,
) -> Result<Vec<SwitchLogEntry>, String> {
    let log = state.switch_log.read().await;
    Ok(log.clone())
}

/// 清除切换日志
#[tauri::command]
pub async fn clear_switch_log(
    state: tauri::State<'_, ResilienceConfigState>,
) -> Result<(), String> {
    let mut log = state.switch_log.write().await;
    log.clear();
    Ok(())
}

/// 添加切换日志条目（内部使用）
#[allow(dead_code)]
pub async fn add_switch_log_entry(
    state: &ResilienceConfigState,
    from_provider: &str,
    to_provider: &str,
    failure_type: &str,
) {
    let mut log = state.switch_log.write().await;
    log.push(SwitchLogEntry {
        from_provider: from_provider.to_string(),
        to_provider: to_provider.to_string(),
        failure_type: failure_type.to_string(),
        timestamp: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    });
    // 保留最近 100 条记录
    if log.len() > 100 {
        log.remove(0);
    }
}
