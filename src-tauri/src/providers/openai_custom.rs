//! OpenAI Custom Provider (自定义 OpenAI 兼容 API)
use crate::models::openai::ChatCompletionRequest;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpenAICustomConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub enabled: bool,
}

pub struct OpenAICustomProvider {
    pub config: OpenAICustomConfig,
    pub client: Client,
}

impl Default for OpenAICustomProvider {
    fn default() -> Self {
        Self {
            config: OpenAICustomConfig::default(),
            client: Client::new(),
        }
    }
}

impl OpenAICustomProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// 使用 API key 和 base_url 创建 Provider
    pub fn with_config(api_key: String, base_url: Option<String>) -> Self {
        Self {
            config: OpenAICustomConfig {
                api_key: Some(api_key),
                base_url,
                enabled: true,
            },
            client: Client::new(),
        }
    }

    pub fn get_base_url(&self) -> String {
        self.config
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.openai.com".to_string())
    }

    pub fn is_configured(&self) -> bool {
        self.config.api_key.is_some() && self.config.enabled
    }

    /// 构建完整的 API URL
    /// 智能处理用户输入的 base_url，无论是否带 /v1 都能正确工作
    fn build_url(&self, endpoint: &str) -> String {
        let base = self.get_base_url();
        let base = base.trim_end_matches('/');

        // 如果用户输入了带 /v1 的 URL，直接拼接 endpoint
        // 否则拼接 /v1/endpoint
        if base.ends_with("/v1") {
            format!("{}/{}", base, endpoint)
        } else {
            format!("{}/v1/{}", base, endpoint)
        }
    }

    /// 调用 OpenAI API（使用类型化请求）
    pub async fn call_api(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<reqwest::Response, Box<dyn Error + Send + Sync>> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or("OpenAI API key not configured")?;

        let url = self.build_url("chat/completions");

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await?;

        Ok(resp)
    }

    pub async fn chat_completions(
        &self,
        request: &serde_json::Value,
    ) -> Result<reqwest::Response, Box<dyn Error + Send + Sync>> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or("OpenAI API key not configured")?;

        let url = self.build_url("chat/completions");

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await?;

        Ok(resp)
    }

    pub async fn list_models(&self) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or("OpenAI API key not configured")?;

        let url = self.build_url("models");

        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {api_key}"))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Failed to list models: {status} - {body}").into());
        }

        let data: serde_json::Value = resp.json().await?;
        Ok(data)
    }
}

// ============================================================================
// StreamingProvider Trait 实现
// ============================================================================

use crate::providers::ProviderError;
use crate::streaming::traits::{
    reqwest_stream_to_stream_response, StreamFormat, StreamResponse, StreamingProvider,
};
use async_trait::async_trait;

#[async_trait]
impl StreamingProvider for OpenAICustomProvider {
    /// 发起流式 API 调用
    ///
    /// 使用 reqwest 的 bytes_stream 返回字节流，支持真正的端到端流式传输。
    /// OpenAI 使用 OpenAI SSE 格式。
    ///
    /// # 需求覆盖
    /// - 需求 1.3: OpenAICustomProvider 流式支持
    async fn call_api_stream(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<StreamResponse, ProviderError> {
        let api_key = self.config.api_key.as_ref().ok_or_else(|| {
            ProviderError::ConfigurationError("OpenAI API key not configured".to_string())
        })?;

        // 确保请求启用流式
        let mut stream_request = request.clone();
        stream_request.stream = true;

        let url = self.build_url("chat/completions");

        tracing::info!(
            "[OPENAI_STREAM] 发起流式请求: url={} model={}",
            url,
            request.model
        );

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .json(&stream_request)
            .send()
            .await
            .map_err(|e| ProviderError::from_reqwest_error(&e))?;

        // 检查响应状态
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            tracing::error!("[OPENAI_STREAM] 请求失败: {} - {}", status, body);
            return Err(ProviderError::from_http_status(status.as_u16(), &body));
        }

        tracing::info!("[OPENAI_STREAM] 流式响应开始: status={}", status);

        // 将 reqwest 响应转换为 StreamResponse
        Ok(reqwest_stream_to_stream_response(resp))
    }

    fn supports_streaming(&self) -> bool {
        self.is_configured()
    }

    fn provider_name(&self) -> &'static str {
        "OpenAICustomProvider"
    }

    fn stream_format(&self) -> StreamFormat {
        StreamFormat::OpenAiSse
    }
}
