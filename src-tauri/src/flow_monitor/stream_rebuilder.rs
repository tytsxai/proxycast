//! SSE 流式响应重建器
//!
//! 该模块负责将分散的 SSE chunks 合并为完整的 LLM 响应。
//! 支持 OpenAI、Anthropic、Gemini 等多种流式响应格式。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

use super::models::{
    LLMResponse, StopReason, StreamChunk, StreamInfo, ThinkingContent, TokenUsage, ToolCall,
    ToolCallDelta,
};

// ============================================================================
// 错误类型
// ============================================================================

/// 流重建错误
#[derive(Debug, Error)]
pub enum StreamRebuilderError {
    /// JSON 解析错误
    #[error("JSON 解析错误: {0}")]
    JsonParseError(#[from] serde_json::Error),

    /// 无效的事件格式
    #[error("无效的事件格式: {0}")]
    InvalidEventFormat(String),

    /// 未知的流格式
    #[error("未知的流格式")]
    UnknownFormat,
}

// ============================================================================
// 流格式枚举
// ============================================================================

/// 流式响应格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamFormat {
    /// OpenAI 格式 (data: {...})
    OpenAI,
    /// Anthropic 格式 (event: xxx, data: {...})
    Anthropic,
    /// Gemini 格式
    Gemini,
    /// 未知格式
    Unknown,
}

impl Default for StreamFormat {
    fn default() -> Self {
        StreamFormat::Unknown
    }
}

// ============================================================================
// 工具调用构建器
// ============================================================================

/// 工具调用构建器，用于累积流式工具调用数据
#[derive(Debug, Clone, Default)]
struct ToolCallBuilder {
    /// 工具调用 ID
    id: Option<String>,
    /// 工具类型
    tool_type: String,
    /// 函数名称
    function_name: Option<String>,
    /// 函数参数（累积的 JSON 字符串）
    arguments: String,
}

impl ToolCallBuilder {
    fn new() -> Self {
        Self {
            id: None,
            tool_type: "function".to_string(),
            function_name: None,
            arguments: String::new(),
        }
    }

    fn build(self) -> Option<ToolCall> {
        let id = self.id?;
        let name = self.function_name?;

        Some(ToolCall {
            id,
            tool_type: self.tool_type,
            function: super::models::FunctionCall {
                name,
                arguments: self.arguments,
            },
        })
    }
}

// ============================================================================
// 流重建器
// ============================================================================

/// SSE 流重建器
///
/// 将分散的 SSE chunks 合并为完整的 LLM 响应。
/// 支持多种流式响应格式的解析和重建。
#[derive(Debug)]
pub struct StreamRebuilder {
    /// 累积的 chunks
    chunks: Vec<StreamChunk>,
    /// 内容缓冲区
    content_buffer: String,
    /// 工具调用构建器（按索引）
    tool_calls_buffer: HashMap<u32, ToolCallBuilder>,
    /// 思维链缓冲区
    thinking_buffer: Option<String>,
    /// 首个 chunk 时间
    first_chunk_time: Option<DateTime<Utc>>,
    /// 最后一个 chunk 时间
    last_chunk_time: Option<DateTime<Utc>>,
    /// 流格式
    format: StreamFormat,
    /// chunk 计数器
    chunk_index: u32,
    /// 停止原因
    stop_reason: Option<StopReason>,
    /// Token 使用量
    usage: TokenUsage,
    /// 响应 ID
    response_id: Option<String>,
    /// 模型名称
    model: Option<String>,
    /// 是否保存原始 chunks
    save_raw_chunks: bool,
    /// 当前内容块索引（Anthropic 格式）
    current_content_block_index: Option<u32>,
    /// 当前内容块类型（Anthropic 格式）
    current_content_block_type: Option<String>,
}

impl StreamRebuilder {
    /// 创建新的流重建器
    pub fn new(format: StreamFormat) -> Self {
        Self {
            chunks: Vec::new(),
            content_buffer: String::new(),
            tool_calls_buffer: HashMap::new(),
            thinking_buffer: None,
            first_chunk_time: None,
            last_chunk_time: None,
            format,
            chunk_index: 0,
            stop_reason: None,
            usage: TokenUsage::default(),
            response_id: None,
            model: None,
            save_raw_chunks: false,
            current_content_block_index: None,
            current_content_block_type: None,
        }
    }

    /// 设置是否保存原始 chunks
    pub fn with_save_raw_chunks(mut self, save: bool) -> Self {
        self.save_raw_chunks = save;
        self
    }

    /// 处理 SSE 事件
    ///
    /// # 参数
    /// - `event`: SSE 事件类型（可选，如 "message", "content_block_delta" 等）
    /// - `data`: SSE 数据内容
    ///
    /// # 返回
    /// - `Ok(())`: 处理成功
    /// - `Err(StreamRebuilderError)`: 处理失败
    pub fn process_event(
        &mut self,
        event: Option<&str>,
        data: &str,
    ) -> Result<(), StreamRebuilderError> {
        let now = Utc::now();

        // 记录时间
        if self.first_chunk_time.is_none() {
            self.first_chunk_time = Some(now);
        }
        self.last_chunk_time = Some(now);

        // 创建 chunk 记录
        let mut chunk = StreamChunk {
            index: self.chunk_index,
            event: event.map(|s| s.to_string()),
            data: data.to_string(),
            timestamp: now,
            content_delta: None,
            tool_call_delta: None,
            thinking_delta: None,
        };

        // 根据格式处理
        let result = match self.format {
            StreamFormat::OpenAI => self.process_openai_chunk(data, &mut chunk),
            StreamFormat::Anthropic => self.process_anthropic_chunk(event, data, &mut chunk),
            StreamFormat::Gemini => self.process_gemini_chunk(data, &mut chunk),
            StreamFormat::Unknown => {
                // 尝试自动检测格式
                if let Some(evt) = event {
                    if evt.starts_with("message_") || evt.starts_with("content_block") {
                        self.format = StreamFormat::Anthropic;
                        self.process_anthropic_chunk(Some(evt), data, &mut chunk)
                    } else {
                        // 尝试 OpenAI 格式
                        self.format = StreamFormat::OpenAI;
                        self.process_openai_chunk(data, &mut chunk)
                    }
                } else {
                    // 尝试 OpenAI 格式
                    self.format = StreamFormat::OpenAI;
                    self.process_openai_chunk(data, &mut chunk)
                }
            }
        };

        // 保存 chunk
        if self.save_raw_chunks {
            self.chunks.push(chunk);
        }

        self.chunk_index += 1;
        result
    }

    /// 处理 OpenAI 格式的 chunk
    ///
    /// OpenAI 流式响应格式:
    /// ```text
    /// data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","created":1234567890,
    ///        "model":"gpt-4","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}
    /// data: [DONE]
    /// ```
    fn process_openai_chunk(
        &mut self,
        data: &str,
        chunk: &mut StreamChunk,
    ) -> Result<(), StreamRebuilderError> {
        let data = data.trim();

        // 处理 [DONE] 终止信号
        if data == "[DONE]" {
            return Ok(());
        }

        // 解析 JSON
        let json: serde_json::Value = serde_json::from_str(data)?;

        // 提取响应 ID 和模型
        if self.response_id.is_none() {
            self.response_id = json
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
        if self.model.is_none() {
            self.model = json
                .get("model")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }

        // 处理 choices
        if let Some(choices) = json.get("choices").and_then(|v| v.as_array()) {
            for choice in choices {
                // 处理 delta
                if let Some(delta) = choice.get("delta") {
                    // 处理内容增量
                    if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                        self.content_buffer.push_str(content);
                        chunk.content_delta = Some(content.to_string());
                    }

                    // 处理工具调用增量
                    if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                        for tc in tool_calls {
                            self.process_openai_tool_call_delta(tc, chunk)?;
                        }
                    }
                }

                // 处理 finish_reason
                if let Some(finish_reason) = choice.get("finish_reason").and_then(|v| v.as_str()) {
                    self.stop_reason = Some(Self::parse_openai_stop_reason(finish_reason));
                }
            }
        }

        // 处理 usage（某些 API 在最后一个 chunk 中包含 usage）
        if let Some(usage) = json.get("usage") {
            self.parse_openai_usage(usage);
        }

        Ok(())
    }

    /// 处理 OpenAI 工具调用增量
    fn process_openai_tool_call_delta(
        &mut self,
        tc: &serde_json::Value,
        chunk: &mut StreamChunk,
    ) -> Result<(), StreamRebuilderError> {
        let index = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

        let builder = self
            .tool_calls_buffer
            .entry(index)
            .or_insert_with(ToolCallBuilder::new);

        // 提取 ID
        if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
            builder.id = Some(id.to_string());
        }

        // 提取函数信息
        if let Some(function) = tc.get("function") {
            if let Some(name) = function.get("name").and_then(|v| v.as_str()) {
                builder.function_name = Some(name.to_string());
            }
            if let Some(args) = function.get("arguments").and_then(|v| v.as_str()) {
                builder.arguments.push_str(args);

                // 记录增量
                chunk.tool_call_delta = Some(ToolCallDelta {
                    index,
                    id: builder.id.clone(),
                    function_name: builder.function_name.clone(),
                    arguments_delta: Some(args.to_string()),
                });
            }
        }

        Ok(())
    }

    /// 解析 OpenAI 停止原因
    fn parse_openai_stop_reason(reason: &str) -> StopReason {
        match reason {
            "stop" => StopReason::Stop,
            "length" => StopReason::Length,
            "tool_calls" => StopReason::ToolCalls,
            "content_filter" => StopReason::ContentFilter,
            "function_call" => StopReason::FunctionCall,
            other => StopReason::Other(other.to_string()),
        }
    }

    /// 解析 OpenAI usage
    fn parse_openai_usage(&mut self, usage: &serde_json::Value) {
        if let Some(prompt_tokens) = usage.get("prompt_tokens").and_then(|v| v.as_u64()) {
            self.usage.input_tokens = prompt_tokens as u32;
        }
        if let Some(completion_tokens) = usage.get("completion_tokens").and_then(|v| v.as_u64()) {
            self.usage.output_tokens = completion_tokens as u32;
        }
        if let Some(total_tokens) = usage.get("total_tokens").and_then(|v| v.as_u64()) {
            self.usage.total_tokens = total_tokens as u32;
        }
    }

    /// 处理 Anthropic 格式的 chunk
    ///
    /// Anthropic 流式响应格式:
    /// ```text
    /// event: message_start
    /// data: {"type":"message_start","message":{"id":"msg_xxx","type":"message","role":"assistant","model":"claude-3"}}
    ///
    /// event: content_block_start
    /// data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}
    ///
    /// event: content_block_delta
    /// data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}
    ///
    /// event: content_block_stop
    /// data: {"type":"content_block_stop","index":0}
    ///
    /// event: message_delta
    /// data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":10}}
    ///
    /// event: message_stop
    /// data: {"type":"message_stop"}
    /// ```
    fn process_anthropic_chunk(
        &mut self,
        event: Option<&str>,
        data: &str,
        chunk: &mut StreamChunk,
    ) -> Result<(), StreamRebuilderError> {
        let data = data.trim();

        // 空数据跳过
        if data.is_empty() {
            return Ok(());
        }

        // 解析 JSON
        let json: serde_json::Value = serde_json::from_str(data)?;

        // 根据事件类型处理
        let event_type = event.or_else(|| json.get("type").and_then(|v| v.as_str()));

        match event_type {
            Some("message_start") => {
                self.process_anthropic_message_start(&json)?;
            }
            Some("content_block_start") => {
                self.process_anthropic_content_block_start(&json)?;
            }
            Some("content_block_delta") => {
                self.process_anthropic_content_block_delta(&json, chunk)?;
            }
            Some("content_block_stop") => {
                self.process_anthropic_content_block_stop(&json)?;
            }
            Some("message_delta") => {
                self.process_anthropic_message_delta(&json)?;
            }
            Some("message_stop") => {
                // 消息结束，无需特殊处理
            }
            Some("ping") => {
                // 心跳，忽略
            }
            Some("error") => {
                // 错误事件
                if let Some(error) = json.get("error") {
                    let msg = error
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown error");
                    return Err(StreamRebuilderError::InvalidEventFormat(msg.to_string()));
                }
            }
            _ => {
                // 未知事件类型，忽略
            }
        }

        Ok(())
    }

    /// 处理 Anthropic message_start 事件
    fn process_anthropic_message_start(
        &mut self,
        json: &serde_json::Value,
    ) -> Result<(), StreamRebuilderError> {
        if let Some(message) = json.get("message") {
            self.response_id = message
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            self.model = message
                .get("model")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            // 处理 usage（input_tokens）
            if let Some(usage) = message.get("usage") {
                if let Some(input_tokens) = usage.get("input_tokens").and_then(|v| v.as_u64()) {
                    self.usage.input_tokens = input_tokens as u32;
                }
            }
        }
        Ok(())
    }

    /// 处理 Anthropic content_block_start 事件
    fn process_anthropic_content_block_start(
        &mut self,
        json: &serde_json::Value,
    ) -> Result<(), StreamRebuilderError> {
        let index = json.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        self.current_content_block_index = Some(index);

        if let Some(content_block) = json.get("content_block") {
            let block_type = content_block
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("text");
            self.current_content_block_type = Some(block_type.to_string());

            match block_type {
                "tool_use" => {
                    // 工具调用开始
                    let builder = self
                        .tool_calls_buffer
                        .entry(index)
                        .or_insert_with(ToolCallBuilder::new);
                    builder.id = content_block
                        .get("id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    builder.function_name = content_block
                        .get("name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                }
                "thinking" => {
                    // 思维链开始
                    if self.thinking_buffer.is_none() {
                        self.thinking_buffer = Some(String::new());
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// 处理 Anthropic content_block_delta 事件
    fn process_anthropic_content_block_delta(
        &mut self,
        json: &serde_json::Value,
        chunk: &mut StreamChunk,
    ) -> Result<(), StreamRebuilderError> {
        let index = json.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

        if let Some(delta) = json.get("delta") {
            let delta_type = delta.get("type").and_then(|v| v.as_str()).unwrap_or("");

            match delta_type {
                "text_delta" => {
                    // 文本增量
                    if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                        self.content_buffer.push_str(text);
                        chunk.content_delta = Some(text.to_string());
                    }
                }
                "thinking_delta" => {
                    // 思维链增量
                    if let Some(thinking) = delta.get("thinking").and_then(|v| v.as_str()) {
                        if let Some(ref mut buffer) = self.thinking_buffer {
                            buffer.push_str(thinking);
                        } else {
                            self.thinking_buffer = Some(thinking.to_string());
                        }
                        chunk.thinking_delta = Some(thinking.to_string());
                    }
                }
                "input_json_delta" => {
                    // 工具调用参数增量
                    if let Some(partial_json) = delta.get("partial_json").and_then(|v| v.as_str()) {
                        if let Some(builder) = self.tool_calls_buffer.get_mut(&index) {
                            builder.arguments.push_str(partial_json);

                            chunk.tool_call_delta = Some(ToolCallDelta {
                                index,
                                id: builder.id.clone(),
                                function_name: builder.function_name.clone(),
                                arguments_delta: Some(partial_json.to_string()),
                            });
                        }
                    }
                }
                "signature_delta" => {
                    // 签名增量（用于思维链验证）
                    // 暂时忽略
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// 处理 Anthropic content_block_stop 事件
    fn process_anthropic_content_block_stop(
        &mut self,
        _json: &serde_json::Value,
    ) -> Result<(), StreamRebuilderError> {
        self.current_content_block_index = None;
        self.current_content_block_type = None;
        Ok(())
    }

    /// 处理 Anthropic message_delta 事件
    fn process_anthropic_message_delta(
        &mut self,
        json: &serde_json::Value,
    ) -> Result<(), StreamRebuilderError> {
        // 处理停止原因
        if let Some(delta) = json.get("delta") {
            if let Some(stop_reason) = delta.get("stop_reason").and_then(|v| v.as_str()) {
                self.stop_reason = Some(Self::parse_anthropic_stop_reason(stop_reason));
            }
        }

        // 处理 usage
        if let Some(usage) = json.get("usage") {
            if let Some(output_tokens) = usage.get("output_tokens").and_then(|v| v.as_u64()) {
                self.usage.output_tokens = output_tokens as u32;
            }
        }

        Ok(())
    }

    /// 解析 Anthropic 停止原因
    fn parse_anthropic_stop_reason(reason: &str) -> StopReason {
        match reason {
            "end_turn" => StopReason::EndTurn,
            "stop_sequence" => StopReason::Stop,
            "max_tokens" => StopReason::Length,
            "tool_use" => StopReason::ToolCalls,
            other => StopReason::Other(other.to_string()),
        }
    }

    /// 处理 Gemini 格式的 chunk
    ///
    /// Gemini 流式响应格式:
    /// ```text
    /// data: {"candidates":[{"content":{"parts":[{"text":"Hello"}],"role":"model"},
    ///        "finishReason":"STOP","index":0}],"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":5}}
    /// ```
    fn process_gemini_chunk(
        &mut self,
        data: &str,
        chunk: &mut StreamChunk,
    ) -> Result<(), StreamRebuilderError> {
        let data = data.trim();

        // 空数据跳过
        if data.is_empty() {
            return Ok(());
        }

        // 解析 JSON
        let json: serde_json::Value = serde_json::from_str(data)?;

        // 处理 candidates
        if let Some(candidates) = json.get("candidates").and_then(|v| v.as_array()) {
            for candidate in candidates {
                // 处理内容
                if let Some(content) = candidate.get("content") {
                    if let Some(parts) = content.get("parts").and_then(|v| v.as_array()) {
                        for part in parts {
                            if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                                self.content_buffer.push_str(text);
                                chunk.content_delta = Some(text.to_string());
                            }

                            // 处理函数调用
                            if let Some(function_call) = part.get("functionCall") {
                                self.process_gemini_function_call(function_call, chunk)?;
                            }
                        }
                    }
                }

                // 处理 finishReason
                if let Some(finish_reason) = candidate.get("finishReason").and_then(|v| v.as_str())
                {
                    self.stop_reason = Some(Self::parse_gemini_stop_reason(finish_reason));
                }
            }
        }

        // 处理 usageMetadata
        if let Some(usage) = json.get("usageMetadata") {
            self.parse_gemini_usage(usage);
        }

        Ok(())
    }

    /// 处理 Gemini 函数调用
    fn process_gemini_function_call(
        &mut self,
        function_call: &serde_json::Value,
        chunk: &mut StreamChunk,
    ) -> Result<(), StreamRebuilderError> {
        let index = self.tool_calls_buffer.len() as u32;
        let builder = self
            .tool_calls_buffer
            .entry(index)
            .or_insert_with(ToolCallBuilder::new);

        // Gemini 的函数调用通常是完整的，不是增量的
        if let Some(name) = function_call.get("name").and_then(|v| v.as_str()) {
            builder.function_name = Some(name.to_string());
            builder.id = Some(format!("call_{}", uuid::Uuid::new_v4()));
        }

        if let Some(args) = function_call.get("args") {
            let args_str = serde_json::to_string(args)?;
            builder.arguments = args_str.clone();

            chunk.tool_call_delta = Some(ToolCallDelta {
                index,
                id: builder.id.clone(),
                function_name: builder.function_name.clone(),
                arguments_delta: Some(args_str),
            });
        }

        Ok(())
    }

    /// 解析 Gemini 停止原因
    fn parse_gemini_stop_reason(reason: &str) -> StopReason {
        match reason {
            "STOP" => StopReason::Stop,
            "MAX_TOKENS" => StopReason::Length,
            "SAFETY" => StopReason::ContentFilter,
            "RECITATION" => StopReason::ContentFilter,
            "FUNCTION_CALL" => StopReason::ToolCalls,
            other => StopReason::Other(other.to_string()),
        }
    }

    /// 解析 Gemini usage
    fn parse_gemini_usage(&mut self, usage: &serde_json::Value) {
        if let Some(prompt_tokens) = usage.get("promptTokenCount").and_then(|v| v.as_u64()) {
            self.usage.input_tokens = prompt_tokens as u32;
        }
        if let Some(candidates_tokens) = usage.get("candidatesTokenCount").and_then(|v| v.as_u64())
        {
            self.usage.output_tokens = candidates_tokens as u32;
        }
        if let Some(total_tokens) = usage.get("totalTokenCount").and_then(|v| v.as_u64()) {
            self.usage.total_tokens = total_tokens as u32;
        }
    }

    /// 完成流重建，返回完整的 LLM 响应
    ///
    /// 合并累积的内容、工具调用、思维链，计算流式统计信息。
    pub fn finish(self) -> LLMResponse {
        let now = Utc::now();

        // 计算流式统计信息
        let stream_info = self.calculate_stream_info();

        // 构建思维链内容
        let thinking = self.thinking_buffer.clone().map(|text| ThinkingContent {
            text,
            tokens: self.usage.thinking_tokens,
            signature: None,
        });

        // 构建工具调用列表
        let mut tool_calls: Vec<ToolCall> = self
            .tool_calls_buffer
            .iter()
            .filter_map(|(_, builder)| builder.clone().build())
            .collect();

        // 按索引排序（如果有多个工具调用）
        tool_calls.sort_by_key(|tc| tc.id.clone());

        // 构建响应体 JSON
        let body = self.build_response_body(&tool_calls, &thinking);

        // 计算 Token 总数
        let mut usage = self.usage.clone();
        usage.calculate_total();

        // 确定时间戳
        let timestamp_start = self.first_chunk_time.unwrap_or(now);
        let timestamp_end = self.last_chunk_time.unwrap_or(now);

        LLMResponse {
            status_code: 200,
            status_text: "OK".to_string(),
            headers: HashMap::new(),
            body,
            content: self.content_buffer,
            thinking,
            tool_calls,
            usage,
            stop_reason: self.stop_reason,
            size_bytes: 0, // 将在外部计算
            timestamp_start,
            timestamp_end,
            stream_info: Some(stream_info),
        }
    }

    /// 计算流式统计信息
    fn calculate_stream_info(&self) -> StreamInfo {
        let chunk_count = self.chunk_index;

        // 计算首个 chunk 延迟
        let first_chunk_latency_ms = 0; // 需要外部提供请求开始时间

        // 计算平均 chunk 间隔
        let avg_chunk_interval_ms = if chunk_count > 1 {
            if let (Some(first), Some(last)) = (self.first_chunk_time, self.last_chunk_time) {
                let total_ms = (last - first).num_milliseconds() as f64;
                total_ms / (chunk_count - 1) as f64
            } else {
                0.0
            }
        } else {
            0.0
        };

        StreamInfo {
            chunk_count,
            first_chunk_latency_ms,
            avg_chunk_interval_ms,
            raw_chunks: if self.save_raw_chunks {
                Some(self.chunks.clone())
            } else {
                None
            },
        }
    }

    /// 构建响应体 JSON
    fn build_response_body(
        &self,
        tool_calls: &[ToolCall],
        thinking: &Option<ThinkingContent>,
    ) -> serde_json::Value {
        match self.format {
            StreamFormat::OpenAI => self.build_openai_response_body(tool_calls),
            StreamFormat::Anthropic => self.build_anthropic_response_body(tool_calls, thinking),
            StreamFormat::Gemini => self.build_gemini_response_body(tool_calls),
            StreamFormat::Unknown => serde_json::json!({
                "content": self.content_buffer,
                "tool_calls": tool_calls,
            }),
        }
    }

    /// 构建 OpenAI 格式响应体
    fn build_openai_response_body(&self, tool_calls: &[ToolCall]) -> serde_json::Value {
        let mut message = serde_json::json!({
            "role": "assistant",
            "content": if self.content_buffer.is_empty() { serde_json::Value::Null } else { serde_json::json!(self.content_buffer) },
        });

        if !tool_calls.is_empty() {
            let tc_json: Vec<serde_json::Value> = tool_calls
                .iter()
                .map(|tc| {
                    serde_json::json!({
                        "id": tc.id,
                        "type": tc.tool_type,
                        "function": {
                            "name": tc.function.name,
                            "arguments": tc.function.arguments,
                        }
                    })
                })
                .collect();
            message["tool_calls"] = serde_json::json!(tc_json);
        }

        serde_json::json!({
            "id": self.response_id.clone().unwrap_or_default(),
            "object": "chat.completion",
            "model": self.model.clone().unwrap_or_default(),
            "choices": [{
                "index": 0,
                "message": message,
                "finish_reason": self.stop_reason.as_ref().map(|r| format!("{:?}", r).to_lowercase()),
            }],
            "usage": {
                "prompt_tokens": self.usage.input_tokens,
                "completion_tokens": self.usage.output_tokens,
                "total_tokens": self.usage.total_tokens,
            }
        })
    }

    /// 构建 Anthropic 格式响应体
    fn build_anthropic_response_body(
        &self,
        tool_calls: &[ToolCall],
        thinking: &Option<ThinkingContent>,
    ) -> serde_json::Value {
        let mut content: Vec<serde_json::Value> = Vec::new();

        // 添加思维链内容
        if let Some(ref thinking_content) = thinking {
            content.push(serde_json::json!({
                "type": "thinking",
                "thinking": thinking_content.text,
            }));
        }

        // 添加文本内容
        if !self.content_buffer.is_empty() {
            content.push(serde_json::json!({
                "type": "text",
                "text": self.content_buffer,
            }));
        }

        // 添加工具调用
        for tc in tool_calls {
            let input: serde_json::Value =
                serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::json!({}));
            content.push(serde_json::json!({
                "type": "tool_use",
                "id": tc.id,
                "name": tc.function.name,
                "input": input,
            }));
        }

        serde_json::json!({
            "id": self.response_id.clone().unwrap_or_default(),
            "type": "message",
            "role": "assistant",
            "model": self.model.clone().unwrap_or_default(),
            "content": content,
            "stop_reason": self.stop_reason.as_ref().map(|r| match r {
                StopReason::EndTurn => "end_turn",
                StopReason::Stop => "stop_sequence",
                StopReason::Length => "max_tokens",
                StopReason::ToolCalls => "tool_use",
                _ => "end_turn",
            }),
            "usage": {
                "input_tokens": self.usage.input_tokens,
                "output_tokens": self.usage.output_tokens,
            }
        })
    }

    /// 构建 Gemini 格式响应体
    fn build_gemini_response_body(&self, tool_calls: &[ToolCall]) -> serde_json::Value {
        let mut parts: Vec<serde_json::Value> = Vec::new();

        // 添加文本内容
        if !self.content_buffer.is_empty() {
            parts.push(serde_json::json!({
                "text": self.content_buffer,
            }));
        }

        // 添加函数调用
        for tc in tool_calls {
            let args: serde_json::Value =
                serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::json!({}));
            parts.push(serde_json::json!({
                "functionCall": {
                    "name": tc.function.name,
                    "args": args,
                }
            }));
        }

        serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": parts,
                    "role": "model",
                },
                "finishReason": self.stop_reason.as_ref().map(|r| match r {
                    StopReason::Stop => "STOP",
                    StopReason::Length => "MAX_TOKENS",
                    StopReason::ContentFilter => "SAFETY",
                    StopReason::ToolCalls => "FUNCTION_CALL",
                    _ => "STOP",
                }),
            }],
            "usageMetadata": {
                "promptTokenCount": self.usage.input_tokens,
                "candidatesTokenCount": self.usage.output_tokens,
                "totalTokenCount": self.usage.total_tokens,
            }
        })
    }

    /// 获取当前格式
    pub fn format(&self) -> StreamFormat {
        self.format
    }

    /// 获取当前内容
    pub fn content(&self) -> &str {
        &self.content_buffer
    }

    /// 获取 chunk 数量
    pub fn chunk_count(&self) -> u32 {
        self.chunk_index
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_simple_stream() {
        let mut rebuilder = StreamRebuilder::new(StreamFormat::OpenAI);

        // 模拟 OpenAI 流式响应
        let chunks = vec![
            r#"{"id":"chatcmpl-123","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}"#,
            r#"{"id":"chatcmpl-123","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#,
            r#"{"id":"chatcmpl-123","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4","choices":[{"index":0,"delta":{"content":" world"},"finish_reason":null}]}"#,
            r#"{"id":"chatcmpl-123","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#,
            "[DONE]",
        ];

        for chunk in chunks {
            rebuilder.process_event(None, chunk).unwrap();
        }

        let response = rebuilder.finish();
        assert_eq!(response.content, "Hello world");
        assert_eq!(response.stop_reason, Some(StopReason::Stop));
    }

    #[test]
    fn test_openai_tool_calls_stream() {
        let mut rebuilder = StreamRebuilder::new(StreamFormat::OpenAI);

        let chunks = vec![
            r#"{"id":"chatcmpl-123","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4","choices":[{"index":0,"delta":{"role":"assistant","content":null,"tool_calls":[{"index":0,"id":"call_abc123","type":"function","function":{"name":"get_weather","arguments":""}}]},"finish_reason":null}]}"#,
            r#"{"id":"chatcmpl-123","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"lo"}}]},"finish_reason":null}]}"#,
            r#"{"id":"chatcmpl-123","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"cation\":"}}]},"finish_reason":null}]}"#,
            r#"{"id":"chatcmpl-123","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"NYC\"}"}}]},"finish_reason":null}]}"#,
            r#"{"id":"chatcmpl-123","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#,
            "[DONE]",
        ];

        for chunk in chunks {
            rebuilder.process_event(None, chunk).unwrap();
        }

        let response = rebuilder.finish();
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].function.name, "get_weather");
        assert_eq!(
            response.tool_calls[0].function.arguments,
            r#"{"location":"NYC"}"#
        );
        assert_eq!(response.stop_reason, Some(StopReason::ToolCalls));
    }

    #[test]
    fn test_anthropic_simple_stream() {
        let mut rebuilder = StreamRebuilder::new(StreamFormat::Anthropic);

        let events = vec![
            (
                "message_start",
                r#"{"type":"message_start","message":{"id":"msg_123","type":"message","role":"assistant","model":"claude-3-opus-20240229","usage":{"input_tokens":10}}}"#,
            ),
            (
                "content_block_start",
                r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
            ),
            (
                "content_block_delta",
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#,
            ),
            (
                "content_block_delta",
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" world"}}"#,
            ),
            (
                "content_block_stop",
                r#"{"type":"content_block_stop","index":0}"#,
            ),
            (
                "message_delta",
                r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5}}"#,
            ),
            ("message_stop", r#"{"type":"message_stop"}"#),
        ];

        for (event, data) in events {
            rebuilder.process_event(Some(event), data).unwrap();
        }

        let response = rebuilder.finish();
        assert_eq!(response.content, "Hello world");
        assert_eq!(response.stop_reason, Some(StopReason::EndTurn));
        assert_eq!(response.usage.input_tokens, 10);
        assert_eq!(response.usage.output_tokens, 5);
    }

    #[test]
    fn test_anthropic_tool_use_stream() {
        let mut rebuilder = StreamRebuilder::new(StreamFormat::Anthropic);

        let events = vec![
            (
                "message_start",
                r#"{"type":"message_start","message":{"id":"msg_123","type":"message","role":"assistant","model":"claude-3","usage":{"input_tokens":10}}}"#,
            ),
            (
                "content_block_start",
                r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_123","name":"get_weather"}}"#,
            ),
            (
                "content_block_delta",
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"loc"}}"#,
            ),
            (
                "content_block_delta",
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"ation\":\"NYC\"}"}}"#,
            ),
            (
                "content_block_stop",
                r#"{"type":"content_block_stop","index":0}"#,
            ),
            (
                "message_delta",
                r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":20}}"#,
            ),
            ("message_stop", r#"{"type":"message_stop"}"#),
        ];

        for (event, data) in events {
            rebuilder.process_event(Some(event), data).unwrap();
        }

        let response = rebuilder.finish();
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "toolu_123");
        assert_eq!(response.tool_calls[0].function.name, "get_weather");
        assert_eq!(
            response.tool_calls[0].function.arguments,
            r#"{"location":"NYC"}"#
        );
        assert_eq!(response.stop_reason, Some(StopReason::ToolCalls));
    }

    #[test]
    fn test_anthropic_thinking_stream() {
        let mut rebuilder = StreamRebuilder::new(StreamFormat::Anthropic);

        let events = vec![
            (
                "message_start",
                r#"{"type":"message_start","message":{"id":"msg_123","type":"message","role":"assistant","model":"claude-3","usage":{"input_tokens":10}}}"#,
            ),
            (
                "content_block_start",
                r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#,
            ),
            (
                "content_block_delta",
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Let me think..."}}"#,
            ),
            (
                "content_block_stop",
                r#"{"type":"content_block_stop","index":0}"#,
            ),
            (
                "content_block_start",
                r#"{"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}"#,
            ),
            (
                "content_block_delta",
                r#"{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"The answer is 42."}}"#,
            ),
            (
                "content_block_stop",
                r#"{"type":"content_block_stop","index":1}"#,
            ),
            (
                "message_delta",
                r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":15}}"#,
            ),
            ("message_stop", r#"{"type":"message_stop"}"#),
        ];

        for (event, data) in events {
            rebuilder.process_event(Some(event), data).unwrap();
        }

        let response = rebuilder.finish();
        assert_eq!(response.content, "The answer is 42.");
        assert!(response.thinking.is_some());
        assert_eq!(response.thinking.unwrap().text, "Let me think...");
    }

    #[test]
    fn test_gemini_simple_stream() {
        let mut rebuilder = StreamRebuilder::new(StreamFormat::Gemini);

        let chunks = vec![
            r#"{"candidates":[{"content":{"parts":[{"text":"Hello"}],"role":"model"},"index":0}]}"#,
            r#"{"candidates":[{"content":{"parts":[{"text":" world"}],"role":"model"},"index":0}]}"#,
            r#"{"candidates":[{"content":{"parts":[{"text":"!"}],"role":"model"},"finishReason":"STOP","index":0}],"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":5,"totalTokenCount":15}}"#,
        ];

        for chunk in chunks {
            rebuilder.process_event(None, chunk).unwrap();
        }

        let response = rebuilder.finish();
        assert_eq!(response.content, "Hello world!");
        assert_eq!(response.stop_reason, Some(StopReason::Stop));
        assert_eq!(response.usage.input_tokens, 10);
        assert_eq!(response.usage.output_tokens, 5);
    }

    #[test]
    fn test_done_signal() {
        let mut rebuilder = StreamRebuilder::new(StreamFormat::OpenAI);

        // [DONE] 信号应该被正确处理
        rebuilder.process_event(None, "[DONE]").unwrap();

        let response = rebuilder.finish();
        assert!(response.content.is_empty());
    }

    #[test]
    fn test_auto_detect_format() {
        // 测试自动检测 Anthropic 格式
        let mut rebuilder = StreamRebuilder::new(StreamFormat::Unknown);
        rebuilder
            .process_event(
                Some("message_start"),
                r#"{"type":"message_start","message":{"id":"msg_123","type":"message","role":"assistant","model":"claude-3","usage":{"input_tokens":10}}}"#,
            )
            .unwrap();
        assert_eq!(rebuilder.format(), StreamFormat::Anthropic);

        // 测试自动检测 OpenAI 格式
        let mut rebuilder = StreamRebuilder::new(StreamFormat::Unknown);
        rebuilder
            .process_event(
                None,
                r#"{"id":"chatcmpl-123","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4","choices":[{"index":0,"delta":{"content":"Hi"},"finish_reason":null}]}"#,
            )
            .unwrap();
        assert_eq!(rebuilder.format(), StreamFormat::OpenAI);
    }

    #[test]
    fn test_stream_info_calculation() {
        let mut rebuilder = StreamRebuilder::new(StreamFormat::OpenAI).with_save_raw_chunks(true);

        let chunks = vec![
            r#"{"id":"chatcmpl-123","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4","choices":[{"index":0,"delta":{"content":"A"},"finish_reason":null}]}"#,
            r#"{"id":"chatcmpl-123","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4","choices":[{"index":0,"delta":{"content":"B"},"finish_reason":null}]}"#,
            r#"{"id":"chatcmpl-123","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4","choices":[{"index":0,"delta":{"content":"C"},"finish_reason":null}]}"#,
            "[DONE]",
        ];

        for chunk in chunks {
            rebuilder.process_event(None, chunk).unwrap();
        }

        let response = rebuilder.finish();
        assert!(response.stream_info.is_some());
        let stream_info = response.stream_info.unwrap();
        assert_eq!(stream_info.chunk_count, 4);
        assert!(stream_info.raw_chunks.is_some());
        assert_eq!(stream_info.raw_chunks.unwrap().len(), 4);
    }
}

// ============================================================================
// 属性测试
// ============================================================================

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    // ========================================================================
    // 生成器
    // ========================================================================

    /// 生成随机的文本内容（用于模拟 LLM 响应）
    fn arb_content() -> impl Strategy<Value = String> {
        prop::collection::vec("[a-zA-Z0-9 .,!?\\n]{1,20}", 1..10).prop_map(|parts| parts.join(""))
    }

    /// 生成随机的工具调用
    fn arb_tool_call() -> impl Strategy<Value = (String, String, String)> {
        (
            "[a-z_]{3,15}",                         // function name
            "[a-f0-9]{8}",                          // call id suffix
            prop::option::of("[a-zA-Z0-9_]{1,20}"), // argument value
        )
            .prop_map(|(name, id_suffix, arg_value)| {
                let id = format!("call_{}", id_suffix);
                let args = match arg_value {
                    Some(val) => format!(r#"{{"value":"{}"}}"#, val),
                    None => "{}".to_string(),
                };
                (id, name, args)
            })
    }

    /// 生成 OpenAI 格式的流式 chunks
    fn generate_openai_chunks(
        content: &str,
        tool_calls: &[(String, String, String)],
    ) -> Vec<String> {
        let mut chunks = Vec::new();
        let model = "gpt-4";
        let id = "chatcmpl-test123";

        // 初始 chunk
        chunks.push(format!(
            r#"{{"id":"{}","object":"chat.completion.chunk","created":1234567890,"model":"{}","choices":[{{"index":0,"delta":{{"role":"assistant","content":""}},"finish_reason":null}}]}}"#,
            id, model
        ));

        // 内容 chunks（每个字符一个 chunk）
        for ch in content.chars() {
            let escaped = match ch {
                '"' => "\\\"".to_string(),
                '\\' => "\\\\".to_string(),
                '\n' => "\\n".to_string(),
                _ => ch.to_string(),
            };
            chunks.push(format!(
                r#"{{"id":"{}","object":"chat.completion.chunk","created":1234567890,"model":"{}","choices":[{{"index":0,"delta":{{"content":"{}"}},"finish_reason":null}}]}}"#,
                id, model, escaped
            ));
        }

        // 工具调用 chunks
        for (idx, (call_id, name, args)) in tool_calls.iter().enumerate() {
            // 工具调用开始
            chunks.push(format!(
                r#"{{"id":"{}","object":"chat.completion.chunk","created":1234567890,"model":"{}","choices":[{{"index":0,"delta":{{"tool_calls":[{{"index":{},"id":"{}","type":"function","function":{{"name":"{}","arguments":""}}}}]}},"finish_reason":null}}]}}"#,
                id, model, idx, call_id, name
            ));

            // 工具调用参数（一次性发送，避免分块导致的转义问题）
            let args_escaped = args.replace('\\', "\\\\").replace('"', "\\\"");
            chunks.push(format!(
                r#"{{"id":"{}","object":"chat.completion.chunk","created":1234567890,"model":"{}","choices":[{{"index":0,"delta":{{"tool_calls":[{{"index":{},"function":{{"arguments":"{}"}}}}]}},"finish_reason":null}}]}}"#,
                id, model, idx, args_escaped
            ));
        }

        // 结束 chunk
        let finish_reason = if tool_calls.is_empty() {
            "stop"
        } else {
            "tool_calls"
        };
        chunks.push(format!(
            r#"{{"id":"{}","object":"chat.completion.chunk","created":1234567890,"model":"{}","choices":[{{"index":0,"delta":{{}},"finish_reason":"{}"}}]}}"#,
            id, model, finish_reason
        ));

        // [DONE] 信号
        chunks.push("[DONE]".to_string());

        chunks
    }

    /// 生成 Anthropic 格式的流式 chunks
    fn generate_anthropic_chunks(
        content: &str,
        tool_calls: &[(String, String, String)],
    ) -> Vec<(String, String)> {
        let mut events = Vec::new();
        let model = "claude-3-opus-20240229";
        let id = "msg_test123";

        // message_start
        events.push((
            "message_start".to_string(),
            format!(
                r#"{{"type":"message_start","message":{{"id":"{}","type":"message","role":"assistant","model":"{}","usage":{{"input_tokens":10}}}}}}"#,
                id, model
            ),
        ));

        // 文本内容块
        if !content.is_empty() {
            events.push((
                "content_block_start".to_string(),
                r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#.to_string(),
            ));

            // 内容 delta（每个字符一个）
            for ch in content.chars() {
                let escaped = match ch {
                    '"' => "\\\"".to_string(),
                    '\\' => "\\\\".to_string(),
                    '\n' => "\\n".to_string(),
                    _ => ch.to_string(),
                };
                events.push((
                    "content_block_delta".to_string(),
                    format!(
                        r#"{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{}"}}}}"#,
                        escaped
                    ),
                ));
            }

            events.push((
                "content_block_stop".to_string(),
                r#"{"type":"content_block_stop","index":0}"#.to_string(),
            ));
        }

        // 工具调用块
        for (idx, (call_id, name, args)) in tool_calls.iter().enumerate() {
            let block_idx = if content.is_empty() { idx } else { idx + 1 };

            events.push((
                "content_block_start".to_string(),
                format!(
                    r#"{{"type":"content_block_start","index":{},"content_block":{{"type":"tool_use","id":"{}","name":"{}"}}}}"#,
                    block_idx, call_id, name
                ),
            ));

            // 参数 delta（一次性发送，避免分块导致的转义问题）
            let args_escaped = args.replace('\\', "\\\\").replace('"', "\\\"");
            events.push((
                "content_block_delta".to_string(),
                format!(
                    r#"{{"type":"content_block_delta","index":{},"delta":{{"type":"input_json_delta","partial_json":"{}"}}}}"#,
                    block_idx, args_escaped
                ),
            ));

            events.push((
                "content_block_stop".to_string(),
                format!(r#"{{"type":"content_block_stop","index":{}}}"#, block_idx),
            ));
        }

        // message_delta
        let stop_reason = if tool_calls.is_empty() {
            "end_turn"
        } else {
            "tool_use"
        };
        events.push((
            "message_delta".to_string(),
            format!(
                r#"{{"type":"message_delta","delta":{{"stop_reason":"{}"}},"usage":{{"output_tokens":20}}}}"#,
                stop_reason
            ),
        ));

        // message_stop
        events.push((
            "message_stop".to_string(),
            r#"{"type":"message_stop"}"#.to_string(),
        ));

        events
    }

    /// 生成 Gemini 格式的流式 chunks
    fn generate_gemini_chunks(content: &str) -> Vec<String> {
        let mut chunks = Vec::new();

        // 内容 chunks（每 5 个字符一个 chunk）
        for chunk_str in content.as_bytes().chunks(5) {
            let chunk_content = String::from_utf8_lossy(chunk_str);
            let escaped = chunk_content
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n");
            chunks.push(format!(
                r#"{{"candidates":[{{"content":{{"parts":[{{"text":"{}"}}],"role":"model"}},"index":0}}]}}"#,
                escaped
            ));
        }

        // 最后一个 chunk 包含 finishReason 和 usage
        if chunks.is_empty() {
            chunks.push(
                r#"{"candidates":[{"content":{"parts":[],"role":"model"},"finishReason":"STOP","index":0}],"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":5,"totalTokenCount":15}}"#.to_string()
            );
        } else {
            // 修改最后一个 chunk 添加 finishReason
            let last = chunks.pop().unwrap();
            let modified = last.replace(
                r#""index":0}]}"#,
                r#""finishReason":"STOP","index":0}],"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":5,"totalTokenCount":15}}"#
            );
            chunks.push(modified);
        }

        chunks
    }

    // ========================================================================
    // 属性测试
    // ========================================================================

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Feature: llm-flow-monitor, Property 2: 流式响应重建 Round-Trip**
        /// **Validates: Requirements 1.4, 1.5, 2.1, 2.2, 2.3**
        ///
        /// *对于任意* 有效的 LLM 响应内容，将其拆分为 SSE chunks 后通过 Stream_Rebuilder 重建，
        /// 重建后的内容应该与原始内容等价（包括文本内容、工具调用和思维链）。
        #[test]
        fn prop_openai_stream_roundtrip(
            content in arb_content(),
        ) {
            // 生成 OpenAI 格式的 chunks
            let chunks = generate_openai_chunks(&content, &[]);

            // 重建
            let mut rebuilder = StreamRebuilder::new(StreamFormat::OpenAI);
            for chunk in chunks {
                rebuilder.process_event(None, &chunk).unwrap();
            }
            let response = rebuilder.finish();

            // 验证内容一致
            prop_assert_eq!(
                response.content,
                content,
                "OpenAI 流式重建后的内容应该与原始内容一致"
            );

            // 验证停止原因
            prop_assert_eq!(
                response.stop_reason,
                Some(StopReason::Stop),
                "无工具调用时停止原因应该是 Stop"
            );
        }

        /// **Feature: llm-flow-monitor, Property 2b: OpenAI 工具调用流式重建**
        /// **Validates: Requirements 1.4, 1.5, 2.1**
        #[test]
        fn prop_openai_tool_calls_roundtrip(
            tool_call in arb_tool_call(),
        ) {
            let (call_id, name, args) = tool_call;
            let tool_calls = vec![(call_id.clone(), name.clone(), args.clone())];

            // 生成 OpenAI 格式的 chunks
            let chunks = generate_openai_chunks("", &tool_calls);

            // 重建
            let mut rebuilder = StreamRebuilder::new(StreamFormat::OpenAI);
            for chunk in chunks {
                rebuilder.process_event(None, &chunk).unwrap();
            }
            let response = rebuilder.finish();

            // 验证工具调用
            prop_assert_eq!(
                response.tool_calls.len(),
                1,
                "应该有一个工具调用"
            );
            prop_assert_eq!(
                &response.tool_calls[0].id,
                &call_id,
                "工具调用 ID 应该一致"
            );
            prop_assert_eq!(
                &response.tool_calls[0].function.name,
                &name,
                "函数名称应该一致"
            );
            prop_assert_eq!(
                &response.tool_calls[0].function.arguments,
                &args,
                "函数参数应该一致"
            );
            prop_assert_eq!(
                response.stop_reason,
                Some(StopReason::ToolCalls),
                "有工具调用时停止原因应该是 ToolCalls"
            );
        }

        /// **Feature: llm-flow-monitor, Property 2c: Anthropic 流式重建**
        /// **Validates: Requirements 1.4, 1.5, 2.2**
        #[test]
        fn prop_anthropic_stream_roundtrip(
            content in arb_content(),
        ) {
            // 生成 Anthropic 格式的 events
            let events = generate_anthropic_chunks(&content, &[]);

            // 重建
            let mut rebuilder = StreamRebuilder::new(StreamFormat::Anthropic);
            for (event, data) in events {
                rebuilder.process_event(Some(&event), &data).unwrap();
            }
            let response = rebuilder.finish();

            // 验证内容一致
            prop_assert_eq!(
                response.content,
                content,
                "Anthropic 流式重建后的内容应该与原始内容一致"
            );

            // 验证停止原因
            prop_assert_eq!(
                response.stop_reason,
                Some(StopReason::EndTurn),
                "无工具调用时停止原因应该是 EndTurn"
            );
        }

        /// **Feature: llm-flow-monitor, Property 2d: Anthropic 工具调用流式重建**
        /// **Validates: Requirements 1.4, 1.5, 2.2**
        #[test]
        fn prop_anthropic_tool_calls_roundtrip(
            tool_call in arb_tool_call(),
        ) {
            let (call_id, name, args) = tool_call;
            let tool_calls = vec![(call_id.clone(), name.clone(), args.clone())];

            // 生成 Anthropic 格式的 events
            let events = generate_anthropic_chunks("", &tool_calls);

            // 重建
            let mut rebuilder = StreamRebuilder::new(StreamFormat::Anthropic);
            for (event, data) in events {
                rebuilder.process_event(Some(&event), &data).unwrap();
            }
            let response = rebuilder.finish();

            // 验证工具调用
            prop_assert_eq!(
                response.tool_calls.len(),
                1,
                "应该有一个工具调用"
            );
            prop_assert_eq!(
                &response.tool_calls[0].id,
                &call_id,
                "工具调用 ID 应该一致"
            );
            prop_assert_eq!(
                &response.tool_calls[0].function.name,
                &name,
                "函数名称应该一致"
            );
            prop_assert_eq!(
                &response.tool_calls[0].function.arguments,
                &args,
                "函数参数应该一致"
            );
            prop_assert_eq!(
                response.stop_reason,
                Some(StopReason::ToolCalls),
                "有工具调用时停止原因应该是 ToolCalls"
            );
        }

        /// **Feature: llm-flow-monitor, Property 2e: Gemini 流式重建**
        /// **Validates: Requirements 1.4, 1.5, 2.3**
        #[test]
        fn prop_gemini_stream_roundtrip(
            content in arb_content(),
        ) {
            // 生成 Gemini 格式的 chunks
            let chunks = generate_gemini_chunks(&content);

            // 重建
            let mut rebuilder = StreamRebuilder::new(StreamFormat::Gemini);
            for chunk in chunks {
                rebuilder.process_event(None, &chunk).unwrap();
            }
            let response = rebuilder.finish();

            // 验证内容一致
            prop_assert_eq!(
                response.content,
                content,
                "Gemini 流式重建后的内容应该与原始内容一致"
            );

            // 验证停止原因
            prop_assert_eq!(
                response.stop_reason,
                Some(StopReason::Stop),
                "停止原因应该是 Stop"
            );
        }

        /// **Feature: llm-flow-monitor, Property 2f: 流式统计信息正确性**
        /// **Validates: Requirements 1.5**
        #[test]
        fn prop_stream_info_correctness(
            content in arb_content(),
        ) {
            // 生成 OpenAI 格式的 chunks
            let chunks = generate_openai_chunks(&content, &[]);
            let expected_chunk_count = chunks.len() as u32;

            // 重建（保存原始 chunks）
            let mut rebuilder = StreamRebuilder::new(StreamFormat::OpenAI).with_save_raw_chunks(true);
            for chunk in &chunks {
                rebuilder.process_event(None, chunk).unwrap();
            }
            let response = rebuilder.finish();

            // 验证流式统计信息
            prop_assert!(response.stream_info.is_some(), "应该有流式统计信息");
            let stream_info = response.stream_info.unwrap();

            prop_assert_eq!(
                stream_info.chunk_count,
                expected_chunk_count,
                "chunk 数量应该正确"
            );

            prop_assert!(stream_info.raw_chunks.is_some(), "应该保存原始 chunks");
            prop_assert_eq!(
                stream_info.raw_chunks.unwrap().len(),
                expected_chunk_count as usize,
                "保存的 chunks 数量应该正确"
            );
        }
    }
}
