//! Codex CLI 配置读取与解析
//!
//! 从 ~/.codex/auth.json 与 ~/.codex/config.toml 读取配置，
//! 解析凭证与模型信息，转换为 ProxyCast 凭证池可用的数据结构。

use crate::models::{CredentialData, PoolProviderType, ProviderCredential};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;

/// Codex CLI 配置解析结果
#[derive(Debug, Clone)]
pub struct CodexCliConfig {
    /// 转换后的凭证（可直接用于凭证池）
    pub credential: ProviderCredential,
    /// 解析出的 API Key（如 OPENAI_API_KEY / YUNYI_API_KEY）
    pub api_key: Option<String>,
    /// 解析出的 API Base URL
    pub api_base_url: Option<String>,
    /// Codex CLI 当前选中的 model_provider
    pub model_provider: Option<String>,
    /// Codex CLI 当前选中的 model
    pub model: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct CodexAuthJson {
    #[serde(
        default,
        alias = "OPENAI_API_KEY",
        alias = "openai_api_key",
        alias = "api_key",
        alias = "apiKey"
    )]
    api_key: Option<String>,
    #[serde(default, alias = "YUNYI_API_KEY", alias = "yunyi_api_key")]
    yunyi_api_key: Option<String>,
    #[serde(default, alias = "api_base_url", alias = "apiBaseUrl")]
    api_base_url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct CodexConfigToml {
    #[serde(default)]
    model_provider: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    model_reasoning_effort: Option<String>,
    #[serde(default)]
    disable_response_storage: Option<bool>,
    #[serde(default)]
    preferred_auth_method: Option<String>,
    #[serde(default)]
    search: Option<bool>,
    #[serde(default)]
    features: Option<HashMap<String, serde_json::Value>>,
    #[serde(default)]
    model_providers: HashMap<String, CodexModelProvider>,
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct CodexModelProvider {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    wire_api: Option<String>,
    #[serde(default)]
    experimental_bearer_token: Option<String>,
    #[serde(default)]
    requires_openai_auth: Option<bool>,
}

/// 读取并解析 Codex CLI 配置
///
/// # Returns
/// - `Some(CodexCliConfig)`: 成功解析并构建凭证
/// - `None`: 未找到有效配置或解析失败
pub async fn load_codex_cli_config() -> Option<CodexCliConfig> {
    let home = match dirs::home_dir() {
        Some(home) => home,
        None => {
            tracing::warn!("无法定位用户主目录，无法读取 Codex CLI 配置");
            return None;
        }
    };

    let codex_dir = home.join(".codex");
    load_codex_cli_config_from_dir(&codex_dir).await
}

/// 从指定目录读取 Codex CLI 配置（便于测试与复用）
pub(crate) async fn load_codex_cli_config_from_dir(codex_dir: &PathBuf) -> Option<CodexCliConfig> {
    let auth_path = codex_dir.join("auth.json");
    let config_path = codex_dir.join("config.toml");

    if !auth_path.exists() && !config_path.exists() {
        tracing::info!(
            "未找到 Codex CLI 配置文件: {} 或 {}",
            auth_path.display(),
            config_path.display()
        );
        return None;
    }

    let auth = match read_auth_json(&auth_path).await {
        Ok(value) => value,
        Err(e) => {
            tracing::error!("读取 Codex CLI auth.json 失败: {}", e);
            return None;
        }
    };

    let config = match read_config_toml(&config_path).await {
        Ok(value) => value,
        Err(e) => {
            // 容错：config.toml 解析失败时，仍可使用 auth.json 构建最小可用凭证
            tracing::warn!("读取 Codex CLI config.toml 失败，将忽略该文件: {}", e);
            None
        }
    };

    if auth.is_none() {
        tracing::warn!(
            "Codex CLI auth.json 不存在，无法构建凭证: {}",
            auth_path.display()
        );
        return None;
    }

    let model_provider = config.as_ref().and_then(|c| c.model_provider.clone());
    let model = config.as_ref().and_then(|c| c.model.clone());

    tracing::info!("Codex CLI model_provider: {:?}", model_provider);

    let provider_cfg = model_provider
        .as_ref()
        .and_then(|name| config.as_ref().and_then(|c| c.model_providers.get(name)));

    let provider_base_url = provider_cfg.and_then(|p| p.base_url.clone());
    let provider_bearer = provider_cfg.and_then(|p| p.experimental_bearer_token.clone());

    tracing::info!("Codex CLI provider_base_url: {:?}", provider_base_url);

    let mut api_key = auth
        .as_ref()
        .and_then(|a| a.api_key.clone().or(a.yunyi_api_key.clone()));

    if api_key.is_none() && provider_bearer.is_some() {
        tracing::info!("auth.json 未包含 API Key，使用 config.toml 的 experimental_bearer_token");
        api_key = provider_bearer;
    }

    if api_key.is_none() {
        tracing::warn!("Codex CLI 未提供 API Key，可能需要 OAuth token 才能访问");
    }

    // 优先使用 config.toml 的 base_url（真实 API 端点）
    // auth.json 的 api_base_url 通常是本地管理端点，不支持健康检查
    let mut api_base_url = provider_base_url.clone();
    tracing::info!("使用 config.toml 的 base_url: {:?}", api_base_url);

    if api_base_url.is_none() {
        api_base_url = auth.as_ref().and_then(|a| a.api_base_url.clone());
        tracing::info!(
            "config.toml 无 base_url，回退到 auth.json: {:?}",
            api_base_url
        );
    }

    if let (Some(provider_base), Some(auth_base)) = (
        provider_base_url.as_ref(),
        auth.as_ref().and_then(|a| a.api_base_url.as_ref()),
    ) {
        if provider_base != auth_base {
            tracing::info!(
                "Codex CLI base_url 存在差异，优先使用 config.toml: config={}, auth={}",
                provider_base,
                auth_base
            );
        }
    }

    let credential = ProviderCredential::new(
        PoolProviderType::Codex,
        CredentialData::CodexOAuth {
            creds_file_path: auth_path.to_string_lossy().to_string(),
            api_base_url: api_base_url.clone(),
        },
    );

    Some(CodexCliConfig {
        credential,
        api_key,
        api_base_url,
        model_provider,
        model,
    })
}

async fn read_auth_json(path: &PathBuf) -> Result<Option<CodexAuthJson>, String> {
    if !path.exists() {
        tracing::info!("未找到 Codex CLI auth.json: {}", path.display());
        return Ok(None);
    }

    let content = fs::read_to_string(path)
        .await
        .map_err(|e| format!("读取 auth.json 失败: {}", e))?;

    let parsed: CodexAuthJson =
        serde_json::from_str(&content).map_err(|e| format!("解析 auth.json 失败: {}", e))?;

    Ok(Some(parsed))
}

async fn read_config_toml(path: &PathBuf) -> Result<Option<CodexConfigToml>, String> {
    if !path.exists() {
        tracing::info!("未找到 Codex CLI config.toml: {}", path.display());
        return Ok(None);
    }

    let content = fs::read_to_string(path)
        .await
        .map_err(|e| format!("读取 config.toml 失败: {}", e))?;

    let parsed: CodexConfigToml =
        toml::from_str(&content).map_err(|e| format!("解析 config.toml 失败: {}", e))?;

    Ok(Some(parsed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_load_from_dir_prefers_config_base_url_and_auth_api_key() {
        let dir = tempfile::tempdir().expect("tempdir");
        let codex_dir = dir.path().to_path_buf();

        fs::write(
            codex_dir.join("auth.json"),
            r#"{ "api_key": "sk-test", "api_base_url": "http://localhost:1234" }"#,
        )
        .await
        .expect("write auth.json");

        fs::write(
            codex_dir.join("config.toml"),
            r#"
model_provider = "openai"
model = "gpt-5.2"

[model_providers.openai]
base_url = "https://api.example.com/v1"
experimental_bearer_token = "bearer-test"
"#,
        )
        .await
        .expect("write config.toml");

        let cfg = load_codex_cli_config_from_dir(&codex_dir)
            .await
            .expect("should load");

        assert_eq!(cfg.api_key.as_deref(), Some("sk-test"));
        assert_eq!(
            cfg.api_base_url.as_deref(),
            Some("https://api.example.com/v1")
        );

        match cfg.credential.credential {
            CredentialData::CodexOAuth {
                creds_file_path,
                api_base_url,
            } => {
                let path = std::path::Path::new(&creds_file_path);
                assert_eq!(path.file_name().and_then(|s| s.to_str()), Some("auth.json"));
                assert_eq!(api_base_url.as_deref(), Some("https://api.example.com/v1"));
            }
            other => panic!("unexpected credential type: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_load_from_dir_falls_back_to_bearer_token_when_auth_has_no_key() {
        let dir = tempfile::tempdir().expect("tempdir");
        let codex_dir = dir.path().to_path_buf();

        fs::write(
            codex_dir.join("auth.json"),
            r#"{ "api_base_url": "http://localhost:1234" }"#,
        )
        .await
        .expect("write auth.json");

        fs::write(
            codex_dir.join("config.toml"),
            r#"
model_provider = "openai"

[model_providers.openai]
base_url = "https://api.example.com/v1"
experimental_bearer_token = "bearer-test"
"#,
        )
        .await
        .expect("write config.toml");

        let cfg = load_codex_cli_config_from_dir(&codex_dir)
            .await
            .expect("should load");

        assert_eq!(cfg.api_key.as_deref(), Some("bearer-test"));
        assert_eq!(
            cfg.api_base_url.as_deref(),
            Some("https://api.example.com/v1")
        );
    }

    #[tokio::test]
    async fn test_load_from_dir_ignores_invalid_toml() {
        let dir = tempfile::tempdir().expect("tempdir");
        let codex_dir = dir.path().to_path_buf();

        fs::write(
            codex_dir.join("auth.json"),
            r#"{ "openai_api_key": "sk-test", "api_base_url": "https://api.from.auth/v1" }"#,
        )
        .await
        .expect("write auth.json");

        fs::write(codex_dir.join("config.toml"), "not = [valid")
            .await
            .expect("write config.toml");

        let cfg = load_codex_cli_config_from_dir(&codex_dir)
            .await
            .expect("should load");

        assert_eq!(cfg.api_key.as_deref(), Some("sk-test"));
        assert_eq!(
            cfg.api_base_url.as_deref(),
            Some("https://api.from.auth/v1")
        );
    }
}
