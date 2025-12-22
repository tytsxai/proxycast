//! OpenAI 格式转换为 Antigravity (Gemini) 格式
use crate::models::openai::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Antigravity/Gemini 内容部分
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_data: Option<InlineData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<GeminiFunctionCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_response: Option<GeminiFunctionResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineData {
    pub mime_type: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiFunctionCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiFunctionResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub response: serde_json::Value,
}

/// Antigravity/Gemini 内容
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiContent {
    pub role: String,
    pub parts: Vec<GeminiPart>,
}

/// Antigravity/Gemini 工具定义
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiTool {
    pub function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiFunctionDeclaration {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

/// Antigravity/Gemini 生成配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_config: Option<ThinkingConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingConfig {
    pub include_thoughts: bool,
    pub thinking_budget: i32,
}

/// Antigravity 请求体内部结构
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityRequestInner {
    pub contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_config: Option<GeminiGenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<GeminiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_config: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// 生成随机请求 ID
fn generate_request_id() -> String {
    format!("agent-{}", Uuid::new_v4())
}

/// 生成随机会话 ID
fn generate_session_id() -> String {
    let uuid = Uuid::new_v4();
    let bytes = uuid.as_bytes();
    let n: u64 = u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]) % 9_000_000_000_000_000_000;
    format!("-{}", n)
}

/// 模型名称映射
fn model_mapping(model: &str) -> &str {
    match model {
        "claude-sonnet-4-5-thinking" => "claude-sonnet-4-5",
        "claude-opus-4-5" => "claude-opus-4-5-thinking",
        "gemini-2.5-flash-thinking" => "gemini-2.5-flash",
        "gemini-2.5-computer-use-preview-10-2025" => "rev19-uic3-1p",
        "gemini-3-pro-image-preview" => "gemini-3-pro-image",
        "gemini-3-pro-preview" => "gemini-3-pro-high",
        "gemini-claude-sonnet-4-5" => "claude-sonnet-4-5",
        "gemini-claude-sonnet-4-5-thinking" => "claude-sonnet-4-5-thinking",
        _ => model,
    }
}

/// 是否启用思维链
fn is_enable_thinking(model: &str) -> bool {
    model.ends_with("-thinking")
        || model == "gemini-2.5-pro"
        || model.starts_with("gemini-3-pro-")
        || model == "rev19-uic3-1p"
        || model == "gpt-oss-120b-medium"
}

/// 将 OpenAI ChatCompletionRequest 转换为 Antigravity 请求体
pub fn convert_openai_to_antigravity_with_context(
    request: &ChatCompletionRequest,
    project_id: &str,
) -> serde_json::Value {
    let actual_model = model_mapping(&request.model);
    let enable_thinking = is_enable_thinking(&request.model);

    let mut contents: Vec<GeminiContent> = Vec::new();
    let mut system_instruction: Option<GeminiContent> = None;

    // 处理消息
    for msg in &request.messages {
        match msg.role.as_str() {
            "system" => {
                let text = msg.get_content_text();
                if !text.is_empty() {
                    system_instruction = Some(GeminiContent {
                        role: "user".to_string(),
                        parts: vec![GeminiPart {
                            text: Some(text),
                            inline_data: None,
                            function_call: None,
                            function_response: None,
                        }],
                    });
                }
            }
            "user" => {
                let parts = convert_user_content(msg);
                if !parts.is_empty() {
                    contents.push(GeminiContent {
                        role: "user".to_string(),
                        parts,
                    });
                }
            }
            "assistant" => {
                let parts = convert_assistant_content(msg, &contents);
                if !parts.is_empty() {
                    // 检查是否需要合并到上一条 model 消息
                    let should_merge = if let Some(last) = contents.last() {
                        last.role == "model"
                            && msg.tool_calls.is_some()
                            && msg.get_content_text().is_empty()
                    } else {
                        false
                    };

                    if should_merge {
                        if let Some(last) = contents.last_mut() {
                            last.parts.extend(parts);
                        }
                    } else {
                        contents.push(GeminiContent {
                            role: "model".to_string(),
                            parts,
                        });
                    }
                }
            }
            "tool" => {
                // Tool 响应
                let tool_id = msg.tool_call_id.clone().unwrap_or_default();
                let content = msg.get_content_text();

                // 从之前的 model 消息中找到对应的 functionCall name
                let function_name = find_function_name(&contents, &tool_id);

                let response_value = serde_json::json!({ "output": content });

                let function_response = GeminiPart {
                    text: None,
                    inline_data: None,
                    function_call: None,
                    function_response: Some(GeminiFunctionResponse {
                        id: Some(tool_id),
                        name: function_name,
                        response: response_value,
                    }),
                };

                // 检查是否需要合并到上一条 user 消息
                let should_merge = if let Some(last) = contents.last() {
                    last.role == "user" && last.parts.iter().any(|p| p.function_response.is_some())
                } else {
                    false
                };

                if should_merge {
                    if let Some(last) = contents.last_mut() {
                        last.parts.push(function_response);
                    }
                } else {
                    contents.push(GeminiContent {
                        role: "user".to_string(),
                        parts: vec![function_response],
                    });
                }
            }
            _ => {}
        }
    }

    // 构建生成配置
    let generation_config = Some(GeminiGenerationConfig {
        temperature: request.temperature.or(Some(1.0)),
        max_output_tokens: request.max_tokens.map(|t| t as i32).or(Some(8096)),
        top_p: Some(0.85),
        top_k: Some(50),
        stop_sequences: Some(vec![
            "<|user|>".to_string(),
            "<|bot|>".to_string(),
            "<|context_request|>".to_string(),
            "<|endoftext|>".to_string(),
            "<|end_of_turn|>".to_string(),
        ]),
        candidate_count: Some(1),
        thinking_config: Some(ThinkingConfig {
            include_thoughts: enable_thinking,
            thinking_budget: if enable_thinking { 1024 } else { 0 },
        }),
    });

    // 转换工具
    let tools = request.tools.as_ref().map(|tools| {
        tools
            .iter()
            .map(|t| GeminiTool {
                function_declarations: vec![GeminiFunctionDeclaration {
                    name: t.function.name.clone(),
                    description: t.function.description.clone(),
                    parameters: clean_parameters(t.function.parameters.clone()),
                }],
            })
            .collect()
    });

    let tool_config = if tools.is_some() {
        Some(serde_json::json!({
            "functionCallingConfig": {
                "mode": "VALIDATED"
            }
        }))
    } else {
        None
    };

    let inner = AntigravityRequestInner {
        contents,
        system_instruction,
        generation_config,
        tools,
        tool_config,
        session_id: Some(generate_session_id()),
    };

    // 构建完整的 Antigravity 请求体
    serde_json::json!({
        "project": project_id,
        "requestId": generate_request_id(),
        "request": inner,
        "model": actual_model,
        "userAgent": "antigravity"
    })
}

/// 从之前的 model 消息中找到对应的 functionCall name
fn find_function_name(contents: &[GeminiContent], tool_id: &str) -> String {
    for content in contents.iter().rev() {
        if content.role == "model" {
            for part in &content.parts {
                if let Some(fc) = &part.function_call {
                    if fc.id.as_deref() == Some(tool_id) {
                        return fc.name.clone();
                    }
                }
            }
        }
    }
    String::new()
}

/// 清理参数中不需要的字段
fn clean_parameters(params: Option<serde_json::Value>) -> Option<serde_json::Value> {
    params.map(clean_value)
}

fn clean_value(value: serde_json::Value) -> serde_json::Value {
    const EXCLUDED_KEYS: &[&str] = &[
        "$schema",
        "additionalProperties",
        "minLength",
        "maxLength",
        "minItems",
        "maxItems",
        "uniqueItems",
    ];

    match value {
        serde_json::Value::Object(map) => {
            let cleaned: serde_json::Map<String, serde_json::Value> = map
                .into_iter()
                .filter(|(k, _)| !EXCLUDED_KEYS.contains(&k.as_str()))
                .map(|(k, v)| (k, clean_value(v)))
                .collect();
            serde_json::Value::Object(cleaned)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.into_iter().map(clean_value).collect())
        }
        other => other,
    }
}

/// 兼容旧接口
pub fn convert_openai_to_antigravity(request: &ChatCompletionRequest) -> serde_json::Value {
    convert_openai_to_antigravity_with_context(request, "")
}

/// 转换用户消息内容
fn convert_user_content(msg: &ChatMessage) -> Vec<GeminiPart> {
    let mut parts = Vec::new();

    match &msg.content {
        Some(MessageContent::Text(text)) => {
            parts.push(GeminiPart {
                text: Some(text.clone()),
                inline_data: None,
                function_call: None,
                function_response: None,
            });
        }
        Some(MessageContent::Parts(content_parts)) => {
            for part in content_parts {
                match part {
                    ContentPart::Text { text } => {
                        parts.push(GeminiPart {
                            text: Some(text.clone()),
                            inline_data: None,
                            function_call: None,
                            function_response: None,
                        });
                    }
                    ContentPart::ImageUrl { image_url } => {
                        // 处理 base64 图片
                        if let Some((mime, data)) = parse_data_url(&image_url.url) {
                            parts.push(GeminiPart {
                                text: None,
                                inline_data: Some(InlineData {
                                    mime_type: mime,
                                    data,
                                }),
                                function_call: None,
                                function_response: None,
                            });
                        }
                    }
                }
            }
        }
        None => {}
    }

    parts
}

/// 转换助手消息内容
fn convert_assistant_content(msg: &ChatMessage, _contents: &[GeminiContent]) -> Vec<GeminiPart> {
    let mut parts = Vec::new();

    // 文本内容
    let text = msg.get_content_text();
    if !text.is_empty() {
        parts.push(GeminiPart {
            text: Some(text.trim_end().to_string()),
            inline_data: None,
            function_call: None,
            function_response: None,
        });
    }

    // 工具调用
    if let Some(tool_calls) = &msg.tool_calls {
        for tc in tool_calls {
            let args: serde_json::Value =
                serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::json!({}));

            parts.push(GeminiPart {
                text: None,
                inline_data: None,
                function_call: Some(GeminiFunctionCall {
                    id: Some(tc.id.clone()),
                    name: tc.function.name.clone(),
                    args: serde_json::json!({ "query": args }),
                }),
                function_response: None,
            });
        }
    }

    parts
}

/// 解析 data URL
fn parse_data_url(url: &str) -> Option<(String, String)> {
    if url.starts_with("data:") {
        let parts: Vec<&str> = url.splitn(2, ',').collect();
        if parts.len() == 2 {
            let meta = parts[0].strip_prefix("data:")?;
            let mime = meta.split(';').next()?.to_string();
            let data = parts[1].to_string();
            return Some((mime, data));
        }
    }
    None
}

/// 将 Antigravity 响应转换为 OpenAI 格式
pub fn convert_antigravity_to_openai_response(
    antigravity_resp: &serde_json::Value,
    model: &str,
) -> serde_json::Value {
    let mut choices = Vec::new();

    if let Some(candidates) = antigravity_resp
        .get("candidates")
        .and_then(|c| c.as_array())
    {
        for (i, candidate) in candidates.iter().enumerate() {
            let mut content = String::new();
            let mut tool_calls: Vec<serde_json::Value> = Vec::new();

            if let Some(parts) = candidate
                .get("content")
                .and_then(|c| c.get("parts"))
                .and_then(|p| p.as_array())
            {
                for part in parts {
                    if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                        content.push_str(text);
                    }
                    if let Some(fc) = part.get("functionCall") {
                        let call_id = format!("call_{}", &uuid::Uuid::new_v4().to_string()[..8]);
                        tool_calls.push(serde_json::json!({
                            "id": call_id,
                            "type": "function",
                            "function": {
                                "name": fc.get("name").and_then(|n| n.as_str()).unwrap_or(""),
                                "arguments": serde_json::to_string(fc.get("args").unwrap_or(&serde_json::json!({}))).unwrap_or_default()
                            }
                        }));
                    }
                }
            }

            let finish_reason = candidate
                .get("finishReason")
                .and_then(|r| r.as_str())
                .map(|r| match r {
                    "STOP" => "stop",
                    "MAX_TOKENS" => "length",
                    "SAFETY" => "content_filter",
                    "RECITATION" => "content_filter",
                    _ => "stop",
                })
                .unwrap_or("stop");

            let mut message = serde_json::json!({
                "role": "assistant",
                "content": if content.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(content) }
            });

            if !tool_calls.is_empty() {
                message["tool_calls"] = serde_json::json!(tool_calls);
            }

            choices.push(serde_json::json!({
                "index": i,
                "message": message,
                "finish_reason": finish_reason
            }));
        }
    }

    // 构建 usage
    let usage = antigravity_resp.get("usageMetadata").map(|u| {
        serde_json::json!({
            "prompt_tokens": u.get("promptTokenCount").and_then(|t| t.as_i64()).unwrap_or(0),
            "completion_tokens": u.get("candidatesTokenCount").and_then(|t| t.as_i64()).unwrap_or(0),
            "total_tokens": u.get("totalTokenCount").and_then(|t| t.as_i64()).unwrap_or(0)
        })
    });

    let mut response = serde_json::json!({
        "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": model,
        "choices": choices
    });

    if let Some(u) = usage {
        response["usage"] = u;
    }

    response
}
