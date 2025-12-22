//! 参数注入相关命令

use crate::config::{save_config, InjectionRuleConfig, InjectionSettings};
use crate::injection::{InjectionMode, InjectionRule};
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// 注入配置状态
#[allow(dead_code)]
pub struct InjectionConfigState(pub Arc<RwLock<InjectionSettings>>);

/// 注入配置响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectionConfigResponse {
    pub enabled: bool,
    pub rules: Vec<InjectionRuleResponse>,
}

/// 注入规则响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectionRuleResponse {
    pub id: String,
    pub pattern: String,
    pub parameters: serde_json::Value,
    pub mode: InjectionMode,
    pub priority: i32,
    pub enabled: bool,
}

impl From<&InjectionRuleConfig> for InjectionRuleResponse {
    fn from(config: &InjectionRuleConfig) -> Self {
        Self {
            id: config.id.clone(),
            pattern: config.pattern.clone(),
            parameters: config.parameters.clone(),
            mode: config.mode,
            priority: config.priority,
            enabled: config.enabled,
        }
    }
}

impl From<&InjectionRule> for InjectionRuleResponse {
    fn from(rule: &InjectionRule) -> Self {
        Self {
            id: rule.id.clone(),
            pattern: rule.pattern.clone(),
            parameters: rule.parameters.clone(),
            mode: rule.mode,
            priority: rule.priority,
            enabled: rule.enabled,
        }
    }
}

/// 获取注入配置
#[tauri::command]
pub async fn get_injection_config(
    state: tauri::State<'_, AppState>,
) -> Result<InjectionConfigResponse, String> {
    let s = state.read().await;
    Ok(InjectionConfigResponse {
        enabled: s.config.injection.enabled,
        rules: s
            .config
            .injection
            .rules
            .iter()
            .map(InjectionRuleResponse::from)
            .collect(),
    })
}

/// 设置注入启用状态
#[tauri::command]
pub async fn set_injection_enabled(
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let mut s = state.write().await;
    s.config.injection.enabled = enabled;
    save_config(&s.config).map_err(|e| e.to_string())?;
    Ok(())
}

/// 获取所有注入规则
#[tauri::command]
pub async fn get_injection_rules(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<InjectionRuleResponse>, String> {
    let s = state.read().await;
    Ok(s.config
        .injection
        .rules
        .iter()
        .map(InjectionRuleResponse::from)
        .collect())
}

/// 添加注入规则
#[tauri::command]
pub async fn add_injection_rule(
    state: tauri::State<'_, AppState>,
    rule: InjectionRuleResponse,
) -> Result<(), String> {
    let mut s = state.write().await;

    // 检查是否已存在相同 ID 的规则
    if s.config.injection.rules.iter().any(|r| r.id == rule.id) {
        return Err(format!("规则 ID '{}' 已存在", rule.id));
    }

    let config_rule = InjectionRuleConfig {
        id: rule.id,
        pattern: rule.pattern,
        parameters: rule.parameters,
        mode: rule.mode,
        priority: rule.priority,
        enabled: rule.enabled,
    };

    s.config.injection.rules.push(config_rule);
    save_config(&s.config).map_err(|e| e.to_string())?;
    Ok(())
}

/// 移除注入规则
#[tauri::command]
pub async fn remove_injection_rule(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let mut s = state.write().await;

    let pos = s
        .config
        .injection
        .rules
        .iter()
        .position(|r| r.id == id)
        .ok_or_else(|| format!("规则 ID '{}' 不存在", id))?;

    s.config.injection.rules.remove(pos);
    save_config(&s.config).map_err(|e| e.to_string())?;
    Ok(())
}

/// 更新注入规则
#[tauri::command]
pub async fn update_injection_rule(
    state: tauri::State<'_, AppState>,
    id: String,
    rule: InjectionRuleResponse,
) -> Result<(), String> {
    let mut s = state.write().await;

    let pos = s
        .config
        .injection
        .rules
        .iter()
        .position(|r| r.id == id)
        .ok_or_else(|| format!("规则 ID '{}' 不存在", id))?;

    s.config.injection.rules[pos] = InjectionRuleConfig {
        id: rule.id,
        pattern: rule.pattern,
        parameters: rule.parameters,
        mode: rule.mode,
        priority: rule.priority,
        enabled: rule.enabled,
    };

    save_config(&s.config).map_err(|e| e.to_string())?;
    Ok(())
}
