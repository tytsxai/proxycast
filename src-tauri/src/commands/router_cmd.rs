//! 路由配置相关 Tauri 命令

use crate::ProviderType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 模型别名
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelAlias {
    pub alias: String,
    pub actual: String,
}

/// 路由规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRuleDto {
    pub pattern: String,
    pub target_provider: ProviderType,
    pub priority: i32,
    pub enabled: bool,
}

/// 路由配置状态
pub struct RouterConfigState {
    pub aliases: Arc<RwLock<HashMap<String, String>>>,
    pub rules: Arc<RwLock<Vec<RoutingRuleDto>>>,
    pub exclusions: Arc<RwLock<HashMap<ProviderType, Vec<String>>>>,
}

impl Default for RouterConfigState {
    fn default() -> Self {
        Self {
            aliases: Arc::new(RwLock::new(HashMap::new())),
            rules: Arc::new(RwLock::new(Vec::new())),
            exclusions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

/// 获取所有模型别名
#[tauri::command]
pub async fn get_model_aliases(
    state: tauri::State<'_, RouterConfigState>,
) -> Result<Vec<ModelAlias>, String> {
    let aliases = state.aliases.read().await;
    Ok(aliases
        .iter()
        .map(|(alias, actual)| ModelAlias {
            alias: alias.clone(),
            actual: actual.clone(),
        })
        .collect())
}

/// 添加模型别名
#[tauri::command]
pub async fn add_model_alias(
    state: tauri::State<'_, RouterConfigState>,
    alias: String,
    actual: String,
) -> Result<(), String> {
    let mut aliases = state.aliases.write().await;
    aliases.insert(alias, actual);
    Ok(())
}

/// 移除模型别名
#[tauri::command]
pub async fn remove_model_alias(
    state: tauri::State<'_, RouterConfigState>,
    alias: String,
) -> Result<(), String> {
    let mut aliases = state.aliases.write().await;
    aliases.remove(&alias);
    Ok(())
}

/// 获取所有路由规则
#[tauri::command]
pub async fn get_routing_rules(
    state: tauri::State<'_, RouterConfigState>,
) -> Result<Vec<RoutingRuleDto>, String> {
    let rules = state.rules.read().await;
    Ok(rules.clone())
}

/// 添加路由规则
#[tauri::command]
pub async fn add_routing_rule(
    state: tauri::State<'_, RouterConfigState>,
    rule: RoutingRuleDto,
) -> Result<(), String> {
    let mut rules = state.rules.write().await;
    // Check for duplicate pattern
    if rules.iter().any(|r| r.pattern == rule.pattern) {
        return Err("该模式已存在".to_string());
    }
    rules.push(rule);
    // Sort by priority
    rules.sort_by(|a, b| a.priority.cmp(&b.priority));
    Ok(())
}

/// 移除路由规则
#[tauri::command]
pub async fn remove_routing_rule(
    state: tauri::State<'_, RouterConfigState>,
    pattern: String,
) -> Result<(), String> {
    let mut rules = state.rules.write().await;
    rules.retain(|r| r.pattern != pattern);
    Ok(())
}

/// 更新路由规则
#[tauri::command]
pub async fn update_routing_rule(
    state: tauri::State<'_, RouterConfigState>,
    pattern: String,
    rule: RoutingRuleDto,
) -> Result<(), String> {
    let mut rules = state.rules.write().await;
    if let Some(existing) = rules.iter_mut().find(|r| r.pattern == pattern) {
        *existing = rule;
        // Re-sort by priority
        rules.sort_by(|a, b| a.priority.cmp(&b.priority));
        Ok(())
    } else {
        Err("规则不存在".to_string())
    }
}

/// 获取所有排除列表
#[tauri::command]
pub async fn get_exclusions(
    state: tauri::State<'_, RouterConfigState>,
) -> Result<HashMap<ProviderType, Vec<String>>, String> {
    let exclusions = state.exclusions.read().await;
    Ok(exclusions.clone())
}

/// 添加排除模式
#[tauri::command]
pub async fn add_exclusion(
    state: tauri::State<'_, RouterConfigState>,
    provider: ProviderType,
    pattern: String,
) -> Result<(), String> {
    let mut exclusions = state.exclusions.write().await;
    let patterns = exclusions.entry(provider).or_default();
    if !patterns.contains(&pattern) {
        patterns.push(pattern);
    }
    Ok(())
}

/// 移除排除模式
#[tauri::command]
pub async fn remove_exclusion(
    state: tauri::State<'_, RouterConfigState>,
    provider: ProviderType,
    pattern: String,
) -> Result<(), String> {
    let mut exclusions = state.exclusions.write().await;
    if let Some(patterns) = exclusions.get_mut(&provider) {
        patterns.retain(|p| p != &pattern);
    }
    Ok(())
}

/// 设置默认 Provider（路由器专用）
#[tauri::command]
pub async fn set_router_default_provider(_provider: ProviderType) -> Result<(), String> {
    // This would integrate with the main config
    // For now, just acknowledge the request
    Ok(())
}

/// 推荐配置预设
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendedPreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub aliases: Vec<ModelAlias>,
    pub rules: Vec<RoutingRuleDto>,
    /// 客户端路由配置
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_providers: Option<EndpointProvidersConfigDto>,
}

/// 端点 Provider 配置 DTO
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EndpointProvidersConfigDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windsurf: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kiro: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other: Option<String>,
}

/// 获取推荐配置列表
#[tauri::command]
pub async fn get_recommended_presets() -> Result<Vec<RecommendedPreset>, String> {
    Ok(vec![
        RecommendedPreset {
            id: "claude-optimized".to_string(),
            name: "Claude 优化配置".to_string(),
            description: "将所有 Claude 模型请求路由到 Kiro，适合主要使用 Claude 的用户"
                .to_string(),
            aliases: vec![
                // Claude 4.5 系列 (最新)
                ModelAlias {
                    alias: "claude".to_string(),
                    actual: "claude-opus-4-5".to_string(),
                },
                ModelAlias {
                    alias: "opus".to_string(),
                    actual: "claude-opus-4-5".to_string(),
                },
                ModelAlias {
                    alias: "sonnet".to_string(),
                    actual: "claude-sonnet-4-5".to_string(),
                },
                ModelAlias {
                    alias: "haiku".to_string(),
                    actual: "claude-haiku-4-5".to_string(),
                },
                // Claude 4 系列
                ModelAlias {
                    alias: "opus-4".to_string(),
                    actual: "claude-opus-4".to_string(),
                },
                ModelAlias {
                    alias: "sonnet-4".to_string(),
                    actual: "claude-sonnet-4".to_string(),
                },
                // Claude 3.7/3.5 系列 (旧版)
                ModelAlias {
                    alias: "sonnet-3.7".to_string(),
                    actual: "claude-3-7-sonnet-latest".to_string(),
                },
                ModelAlias {
                    alias: "sonnet-3.5".to_string(),
                    actual: "claude-3-5-sonnet-latest".to_string(),
                },
            ],
            rules: vec![
                RoutingRuleDto {
                    pattern: "claude-*".to_string(),
                    target_provider: ProviderType::Kiro,
                    priority: 1,
                    enabled: true,
                },
                RoutingRuleDto {
                    pattern: "*sonnet*".to_string(),
                    target_provider: ProviderType::Kiro,
                    priority: 2,
                    enabled: true,
                },
                RoutingRuleDto {
                    pattern: "*opus*".to_string(),
                    target_provider: ProviderType::Kiro,
                    priority: 2,
                    enabled: true,
                },
                RoutingRuleDto {
                    pattern: "*haiku*".to_string(),
                    target_provider: ProviderType::Kiro,
                    priority: 2,
                    enabled: true,
                },
            ],
            endpoint_providers: None,
        },
        RecommendedPreset {
            id: "gemini-optimized".to_string(),
            name: "Gemini 优化配置".to_string(),
            description: "将 Gemini 模型请求路由到 Gemini Provider，适合主要使用 Google AI 的用户"
                .to_string(),
            aliases: vec![
                // Gemini 3 系列 (最新)
                ModelAlias {
                    alias: "gemini".to_string(),
                    actual: "gemini-3-pro".to_string(),
                },
                ModelAlias {
                    alias: "gemini-pro".to_string(),
                    actual: "gemini-3-pro".to_string(),
                },
                ModelAlias {
                    alias: "gemini-3".to_string(),
                    actual: "gemini-3-pro".to_string(),
                },
                // Gemini 2.5 系列
                ModelAlias {
                    alias: "flash".to_string(),
                    actual: "gemini-2.5-flash".to_string(),
                },
                ModelAlias {
                    alias: "flash-lite".to_string(),
                    actual: "gemini-2.5-flash-lite".to_string(),
                },
                ModelAlias {
                    alias: "gemini-2.5".to_string(),
                    actual: "gemini-2.5-pro".to_string(),
                },
            ],
            rules: vec![
                RoutingRuleDto {
                    pattern: "gemini-*".to_string(),
                    target_provider: ProviderType::Gemini,
                    priority: 1,
                    enabled: true,
                },
                RoutingRuleDto {
                    pattern: "*flash*".to_string(),
                    target_provider: ProviderType::Gemini,
                    priority: 2,
                    enabled: true,
                },
            ],
            endpoint_providers: None,
        },
        RecommendedPreset {
            id: "multi-provider".to_string(),
            name: "多 Provider 均衡配置".to_string(),
            description: "根据模型名称自动路由到对应的 Provider，适合同时使用多个 AI 服务的用户"
                .to_string(),
            aliases: vec![
                // Claude (最新)
                ModelAlias {
                    alias: "claude".to_string(),
                    actual: "claude-opus-4-5".to_string(),
                },
                ModelAlias {
                    alias: "sonnet".to_string(),
                    actual: "claude-sonnet-4-5".to_string(),
                },
                // Gemini (最新)
                ModelAlias {
                    alias: "gemini".to_string(),
                    actual: "gemini-3-pro".to_string(),
                },
                ModelAlias {
                    alias: "flash".to_string(),
                    actual: "gemini-2.5-flash".to_string(),
                },
                // Qwen
                ModelAlias {
                    alias: "qwen".to_string(),
                    actual: "qwen3-coder-plus".to_string(),
                },
                // OpenAI (最新)
                ModelAlias {
                    alias: "gpt".to_string(),
                    actual: "gpt-5.2".to_string(),
                },
                ModelAlias {
                    alias: "gpt-5".to_string(),
                    actual: "gpt-5.2".to_string(),
                },
                ModelAlias {
                    alias: "gpt-4".to_string(),
                    actual: "gpt-4o".to_string(),
                },
                ModelAlias {
                    alias: "o1".to_string(),
                    actual: "o1".to_string(),
                },
                ModelAlias {
                    alias: "o3".to_string(),
                    actual: "o3".to_string(),
                },
            ],
            rules: vec![
                RoutingRuleDto {
                    pattern: "claude-*".to_string(),
                    target_provider: ProviderType::Kiro,
                    priority: 1,
                    enabled: true,
                },
                RoutingRuleDto {
                    pattern: "gemini-*".to_string(),
                    target_provider: ProviderType::Gemini,
                    priority: 1,
                    enabled: true,
                },
                RoutingRuleDto {
                    pattern: "qwen*".to_string(),
                    target_provider: ProviderType::Qwen,
                    priority: 1,
                    enabled: true,
                },
                RoutingRuleDto {
                    pattern: "gpt-*".to_string(),
                    target_provider: ProviderType::OpenAI,
                    priority: 1,
                    enabled: true,
                },
                RoutingRuleDto {
                    pattern: "o1*".to_string(),
                    target_provider: ProviderType::OpenAI,
                    priority: 1,
                    enabled: true,
                },
                RoutingRuleDto {
                    pattern: "o3*".to_string(),
                    target_provider: ProviderType::OpenAI,
                    priority: 1,
                    enabled: true,
                },
            ],
            endpoint_providers: None,
        },
        RecommendedPreset {
            id: "coding-assistant".to_string(),
            name: "编程助手配置".to_string(),
            description:
                "针对编程场景优化，Claude Opus 4.5 用于复杂代码，Gemini Flash 用于快速响应"
                    .to_string(),
            aliases: vec![
                ModelAlias {
                    alias: "code".to_string(),
                    actual: "claude-opus-4-5".to_string(),
                },
                ModelAlias {
                    alias: "coder".to_string(),
                    actual: "qwen3-coder-plus".to_string(),
                },
                ModelAlias {
                    alias: "fast".to_string(),
                    actual: "gemini-2.5-flash".to_string(),
                },
                ModelAlias {
                    alias: "think".to_string(),
                    actual: "claude-sonnet-4-5".to_string(),
                },
            ],
            rules: vec![
                RoutingRuleDto {
                    pattern: "*coder*".to_string(),
                    target_provider: ProviderType::Qwen,
                    priority: 1,
                    enabled: true,
                },
                RoutingRuleDto {
                    pattern: "claude-*".to_string(),
                    target_provider: ProviderType::Kiro,
                    priority: 2,
                    enabled: true,
                },
                RoutingRuleDto {
                    pattern: "gemini-*".to_string(),
                    target_provider: ProviderType::Gemini,
                    priority: 2,
                    enabled: true,
                },
            ],
            endpoint_providers: None,
        },
        RecommendedPreset {
            id: "cost-effective".to_string(),
            name: "性价比优先配置".to_string(),
            description: "优先使用免费或低成本的模型，适合预算有限的用户".to_string(),
            aliases: vec![
                ModelAlias {
                    alias: "default".to_string(),
                    actual: "gemini-2.5-flash".to_string(),
                },
                ModelAlias {
                    alias: "cheap".to_string(),
                    actual: "gemini-2.5-flash-lite".to_string(),
                },
                ModelAlias {
                    alias: "free".to_string(),
                    actual: "gemini-2.5-flash".to_string(),
                },
            ],
            rules: vec![
                // 默认路由到 Gemini（免费额度高）
                RoutingRuleDto {
                    pattern: "*".to_string(),
                    target_provider: ProviderType::Gemini,
                    priority: 100,
                    enabled: true,
                },
                // Claude 请求仍然路由到 Kiro
                RoutingRuleDto {
                    pattern: "claude-*".to_string(),
                    target_provider: ProviderType::Kiro,
                    priority: 1,
                    enabled: true,
                },
            ],
            endpoint_providers: None,
        },
        // 客户端路由预设
        RecommendedPreset {
            id: "client-routing".to_string(),
            name: "客户端路由配置".to_string(),
            description: "为不同的 IDE 客户端配置不同的 Provider，Cursor/Windsurf 使用 Kiro，Claude Code 使用 Kiro，Codex 使用 OpenAI"
                .to_string(),
            aliases: vec![],
            rules: vec![],
            endpoint_providers: Some(EndpointProvidersConfigDto {
                cursor: Some("kiro".to_string()),
                claude_code: Some("kiro".to_string()),
                codex: Some("openai".to_string()),
                windsurf: Some("kiro".to_string()),
                kiro: Some("kiro".to_string()),
                other: None,
            }),
        },
    ])
}

/// 应用推荐配置
#[tauri::command]
pub async fn apply_recommended_preset(
    state: tauri::State<'_, RouterConfigState>,
    preset_id: String,
    merge: bool,
) -> Result<(), String> {
    let presets = get_recommended_presets().await?;
    let preset = presets
        .into_iter()
        .find(|p| p.id == preset_id)
        .ok_or_else(|| format!("未找到预设配置: {}", preset_id))?;

    // 应用别名
    {
        let mut aliases = state.aliases.write().await;
        if !merge {
            aliases.clear();
        }
        for alias in preset.aliases {
            aliases.insert(alias.alias, alias.actual);
        }
    }

    // 应用规则
    {
        let mut rules = state.rules.write().await;
        if !merge {
            rules.clear();
        }
        for rule in preset.rules {
            // 避免重复
            if !rules.iter().any(|r| r.pattern == rule.pattern) {
                rules.push(rule);
            }
        }
        // 按优先级排序
        rules.sort_by(|a, b| a.priority.cmp(&b.priority));
    }

    Ok(())
}

/// 清空所有路由配置
#[tauri::command]
pub async fn clear_all_routing_config(
    state: tauri::State<'_, RouterConfigState>,
) -> Result<(), String> {
    {
        let mut aliases = state.aliases.write().await;
        aliases.clear();
    }
    {
        let mut rules = state.rules.write().await;
        rules.clear();
    }
    {
        let mut exclusions = state.exclusions.write().await;
        exclusions.clear();
    }
    Ok(())
}
