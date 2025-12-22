# LLM Flow Monitor - 详细设计方案

> 参考 mitmproxy 的 Flow 模型，为 ProxyCast 设计一套完整的 LLM API 流量监控系统，
> 用于捕获、存储、分析和回放 AI Agent 与大模型之间的完整交互数据。

## 一、背景与目标

### 1.1 当前问题

1. **日志信息不完整**：当前 `RequestLog` 只记录元数据（id、provider、model、duration、tokens），不保存完整的请求和响应内容
2. **流式响应丢失**：SSE 流式响应的 chunks 分散，无法重建完整的响应内容
3. **无法调试 Agent**：开发 AI Agent 时，需要查看完整的 prompt 和 response 来调优
4. **缺乏历史回放**：无法回放历史请求，难以复现问题
5. **数据不可导出**：无法导出为标准格式（如 HAR）供其他工具分析

### 1.2 设计目标

1. **完整捕获**：记录每个请求的完整 headers、body、响应内容
2. **流式重建**：自动将 SSE chunks 合并为完整响应
3. **高效存储**：内存 + 文件双层存储，支持大量请求
4. **灵活查询**：按时间、模型、provider、内容等多维度过滤
5. **标准导出**：支持 HAR、JSON、Markdown 等格式导出
6. **实时监控**：前端实时展示请求列表和详情
7. **隐私保护**：敏感信息脱敏，可配置存储策略

---

## 二、数据模型设计

### 2.1 核心数据结构

```rust
/// LLM 请求/响应流
/// 类似 mitmproxy 的 HTTPFlow，但专门针对 LLM API 优化
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMFlow {
    /// 唯一标识符
    pub id: String,
    
    /// 流类型
    pub flow_type: FlowType,
    
    /// 请求信息
    pub request: LLMRequest,
    
    /// 响应信息（可能为空，如请求失败）
    pub response: Option<LLMResponse>,
    
    /// 错误信息（如果发生错误）
    pub error: Option<FlowError>,
    
    /// 元数据
    pub metadata: FlowMetadata,
    
    /// 时间戳
    pub timestamps: FlowTimestamps,
    
    /// 流状态
    pub state: FlowState,
    
    /// 用户标记和注释
    pub annotations: FlowAnnotations,
}

/// 流类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FlowType {
    /// OpenAI Chat Completions
    ChatCompletions,
    /// Anthropic Messages
    AnthropicMessages,
    /// Gemini Generate Content
    GeminiGenerateContent,
    /// Embeddings
    Embeddings,
    /// 其他
    Other(String),
}

/// 流状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FlowState {
    /// 等待响应
    Pending,
    /// 正在流式传输
    Streaming,
    /// 已完成
    Completed,
    /// 失败
    Failed,
    /// 已取消
    Cancelled,
    /// 已拦截（用于调试）
    Intercepted,
}
```

### 2.2 请求数据结构

```rust
/// LLM 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMRequest {
    /// HTTP 方法
    pub method: String,
    
    /// 请求路径
    pub path: String,
    
    /// 请求头
    pub headers: HashMap<String, String>,
    
    /// 原始请求体（JSON）
    pub body: serde_json::Value,
    
    /// 解析后的消息列表
    pub messages: Vec<Message>,
    
    /// 系统提示词（如果有）
    pub system_prompt: Option<String>,
    
    /// 工具定义（如果有）
    pub tools: Option<Vec<ToolDefinition>>,
    
    /// 请求的模型名称
    pub model: String,
    
    /// 原始模型名称（别名解析前）
    pub original_model: Option<String>,
    
    /// 请求参数
    pub parameters: RequestParameters,
    
    /// 请求体大小（字节）
    pub size_bytes: usize,
    
    /// 请求开始时间戳
    pub timestamp: DateTime<Utc>,
}

/// 消息结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// 角色
    pub role: MessageRole,
    
    /// 内容（可以是文本或多模态）
    pub content: MessageContent,
    
    /// 工具调用（assistant 消息）
    pub tool_calls: Option<Vec<ToolCall>>,
    
    /// 工具结果（tool 消息）
    pub tool_result: Option<ToolResult>,
    
    /// 消息名称（function/tool 消息）
    pub name: Option<String>,
}

/// 消息角色
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
    Function,
}

/// 消息内容（支持多模态）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// 纯文本
    Text(String),
    
    /// 多模态内容
    MultiModal(Vec<ContentPart>),
}

/// 内容部分
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    /// 文本
    #[serde(rename = "text")]
    Text { text: String },
    
    /// 图片
    #[serde(rename = "image_url")]
    Image { 
        image_url: ImageUrl,
        /// 图片摘要（用于显示，不存储完整 base64）
        #[serde(skip_serializing_if = "Option::is_none")]
        thumbnail: Option<String>,
    },
    
    /// 音频
    #[serde(rename = "audio")]
    Audio { 
        audio: AudioData,
    },
    
    /// 文件
    #[serde(rename = "file")]
    File {
        file: FileData,
    },
}

/// 请求参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequestParameters {
    /// 温度
    pub temperature: Option<f32>,
    /// Top P
    pub top_p: Option<f32>,
    /// 最大 tokens
    pub max_tokens: Option<u32>,
    /// 停止序列
    pub stop: Option<Vec<String>>,
    /// 是否流式
    pub stream: bool,
    /// 其他参数
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}
```

### 2.3 响应数据结构

```rust
/// LLM 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMResponse {
    /// HTTP 状态码
    pub status_code: u16,
    
    /// 状态文本
    pub status_text: String,
    
    /// 响应头
    pub headers: HashMap<String, String>,
    
    /// 原始响应体（完整 JSON，流式响应会被重建）
    pub body: serde_json::Value,
    
    /// 提取的文本内容
    pub content: String,
    
    /// 思维链内容（如果有）
    pub thinking: Option<ThinkingContent>,
    
    /// 工具调用（如果有）
    pub tool_calls: Vec<ToolCall>,
    
    /// Token 使用统计
    pub usage: TokenUsage,
    
    /// 停止原因
    pub stop_reason: Option<StopReason>,
    
    /// 响应体大小（字节）
    pub size_bytes: usize,
    
    /// 响应开始时间戳
    pub timestamp_start: DateTime<Utc>,
    
    /// 响应结束时间戳
    pub timestamp_end: DateTime<Utc>,
    
    /// 流式响应信息（如果是流式）
    pub stream_info: Option<StreamInfo>,
}

/// 思维链内容
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingContent {
    /// 思维链文本
    pub text: String,
    /// 思维链 token 数
    pub tokens: Option<u32>,
    /// 思维链签名（用于验证）
    pub signature: Option<String>,
}

/// 工具调用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// 调用 ID
    pub id: String,
    /// 工具类型
    pub call_type: String,
    /// 函数信息
    pub function: FunctionCall,
}

/// 函数调用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// 函数名
    pub name: String,
    /// 参数（JSON 字符串）
    pub arguments: String,
    /// 解析后的参数（方便查看）
    pub parsed_arguments: Option<serde_json::Value>,
}

/// Token 使用统计
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    /// 输入 tokens
    pub input_tokens: u32,
    /// 输出 tokens
    pub output_tokens: u32,
    /// 缓存读取 tokens
    pub cache_read_tokens: Option<u32>,
    /// 缓存写入 tokens
    pub cache_write_tokens: Option<u32>,
    /// 思维链 tokens
    pub thinking_tokens: Option<u32>,
    /// 总 tokens
    pub total_tokens: u32,
}

/// 停止原因
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StopReason {
    /// 正常结束
    Stop,
    /// 达到长度限制
    Length,
    /// 工具调用
    ToolUse,
    /// 内容过滤
    ContentFilter,
    /// 其他
    Other(String),
}

/// 流式响应信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamInfo {
    /// 总 chunk 数
    pub chunk_count: u32,
    /// 第一个 chunk 延迟（毫秒）
    pub first_chunk_latency_ms: u64,
    /// 平均 chunk 间隔（毫秒）
    pub avg_chunk_interval_ms: f64,
    /// 原始 chunks（可选保存）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_chunks: Option<Vec<StreamChunk>>,
}

/// 流式 chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    /// 序号
    pub index: u32,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// 原始数据
    pub data: String,
    /// 增量内容
    pub delta_content: Option<String>,
}
```

### 2.4 元数据结构

```rust
/// 流元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowMetadata {
    /// Provider 类型
    pub provider: ProviderType,
    
    /// 使用的凭证 ID
    pub credential_id: Option<String>,
    
    /// 凭证名称（用于显示）
    pub credential_name: Option<String>,
    
    /// 重试次数
    pub retry_count: u32,
    
    /// 客户端信息
    pub client_info: ClientInfo,
    
    /// 路由信息
    pub routing_info: RoutingInfo,
    
    /// 注入的参数
    pub injected_params: Option<HashMap<String, serde_json::Value>>,
    
    /// 上下文使用率（%）
    pub context_usage_percentage: Option<f32>,
}

/// 客户端信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    /// 客户端 IP
    pub ip: Option<String>,
    /// User-Agent
    pub user_agent: Option<String>,
    /// 客户端 SDK
    pub sdk: Option<String>,
    /// 客户端版本
    pub sdk_version: Option<String>,
}

/// 路由信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingInfo {
    /// 原始模型（别名）
    pub original_model: String,
    /// 解析后的模型
    pub resolved_model: String,
    /// 路由到的 Provider
    pub routed_provider: ProviderType,
    /// 匹配的路由规则
    pub matched_rule: Option<String>,
}

/// 时间戳集合
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowTimestamps {
    /// 请求创建时间
    pub created: DateTime<Utc>,
    /// 请求发送时间
    pub request_start: DateTime<Utc>,
    /// 请求发送完成时间
    pub request_end: Option<DateTime<Utc>>,
    /// 响应开始时间（收到第一个字节）
    pub response_start: Option<DateTime<Utc>>,
    /// 响应结束时间
    pub response_end: Option<DateTime<Utc>>,
    /// 总耗时（毫秒）
    pub duration_ms: u64,
    /// TTFB（Time To First Byte，毫秒）
    pub ttfb_ms: Option<u64>,
}

/// 用户标注
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FlowAnnotations {
    /// 用户标记（如 ⭐、🔴、🟢）
    pub marker: Option<String>,
    /// 用户备注
    pub comment: Option<String>,
    /// 标签
    pub tags: Vec<String>,
    /// 是否已收藏
    pub starred: bool,
}
```

### 2.5 错误结构

```rust
/// 流错误
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowError {
    /// 错误类型
    pub error_type: FlowErrorType,
    /// 错误消息
    pub message: String,
    /// HTTP 状态码（如果有）
    pub status_code: Option<u16>,
    /// 原始错误响应
    pub raw_response: Option<String>,
    /// 错误发生时间
    pub timestamp: DateTime<Utc>,
    /// 是否可重试
    pub retryable: bool,
}

/// 错误类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FlowErrorType {
    /// 网络错误
    Network,
    /// 超时
    Timeout,
    /// 认证失败
    Authentication,
    /// 限流
    RateLimit,
    /// 内容过滤
    ContentFilter,
    /// 服务端错误
    ServerError,
    /// 请求格式错误
    BadRequest,
    /// 模型不可用
    ModelUnavailable,
    /// Token 超限
    TokenLimitExceeded,
    /// 其他
    Other,
}
```

---

## 三、流式响应重建

### 3.1 SSE 解析器

```rust
/// SSE 流重建器
pub struct StreamRebuilder {
    /// 累积的 chunks
    chunks: Vec<StreamChunk>,
    /// 累积的内容
    content_buffer: String,
    /// 累积的 tool calls
    tool_calls_buffer: HashMap<String, ToolCallBuilder>,
    /// 累积的 thinking
    thinking_buffer: Option<String>,
    /// 第一个 chunk 时间
    first_chunk_time: Option<DateTime<Utc>>,
    /// 上一个 chunk 时间
    last_chunk_time: Option<DateTime<Utc>>,
    /// 流格式
    format: StreamFormat,
}

/// 流格式
pub enum StreamFormat {
    /// OpenAI 格式
    OpenAI,
    /// Anthropic 格式
    Anthropic,
    /// Gemini 格式
    Gemini,
    /// 未知格式
    Unknown,
}

impl StreamRebuilder {
    /// 处理一个 SSE 事件
    pub fn process_event(&mut self, event: &str, data: &str) -> Result<(), Error> {
        let chunk = StreamChunk {
            index: self.chunks.len() as u32,
            timestamp: Utc::now(),
            data: data.to_string(),
            delta_content: None,
        };
        
        // 根据格式解析增量内容
        match self.format {
            StreamFormat::OpenAI => self.process_openai_chunk(data, &mut chunk)?,
            StreamFormat::Anthropic => self.process_anthropic_chunk(event, data, &mut chunk)?,
            StreamFormat::Gemini => self.process_gemini_chunk(data, &mut chunk)?,
            _ => {},
        }
        
        self.chunks.push(chunk);
        Ok(())
    }
    
    /// 完成重建，返回完整响应
    pub fn finish(self) -> LLMResponse {
        // 构建完整的响应对象
        LLMResponse {
            content: self.content_buffer,
            tool_calls: self.tool_calls_buffer.into_values().map(|b| b.build()).collect(),
            thinking: self.thinking_buffer.map(|t| ThinkingContent { text: t, tokens: None, signature: None }),
            stream_info: Some(StreamInfo {
                chunk_count: self.chunks.len() as u32,
                first_chunk_latency_ms: self.calculate_first_chunk_latency(),
                avg_chunk_interval_ms: self.calculate_avg_interval(),
                raw_chunks: if self.should_save_raw_chunks() { Some(self.chunks) } else { None },
            }),
            // ... 其他字段
        }
    }
}
```

### 3.2 不同格式处理

```rust
impl StreamRebuilder {
    /// 处理 OpenAI 格式的 chunk
    fn process_openai_chunk(&mut self, data: &str, chunk: &mut StreamChunk) -> Result<(), Error> {
        if data == "[DONE]" {
            return Ok(());
        }
        
        let parsed: OpenAIStreamChunk = serde_json::from_str(data)?;
        
        for choice in &parsed.choices {
            if let Some(delta) = &choice.delta {
                // 文本内容
                if let Some(content) = &delta.content {
                    self.content_buffer.push_str(content);
                    chunk.delta_content = Some(content.clone());
                }
                
                // 工具调用
                if let Some(tool_calls) = &delta.tool_calls {
                    for tc in tool_calls {
                        self.process_tool_call_delta(tc);
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// 处理 Anthropic 格式的 chunk
    fn process_anthropic_chunk(&mut self, event: &str, data: &str, chunk: &mut StreamChunk) -> Result<(), Error> {
        match event {
            "content_block_delta" => {
                let parsed: AnthropicDelta = serde_json::from_str(data)?;
                match &parsed.delta {
                    Delta::TextDelta { text } => {
                        self.content_buffer.push_str(text);
                        chunk.delta_content = Some(text.clone());
                    },
                    Delta::ThinkingDelta { thinking } => {
                        self.thinking_buffer.get_or_insert(String::new()).push_str(thinking);
                    },
                    Delta::InputJsonDelta { partial_json } => {
                        // 处理工具调用参数
                        self.process_tool_call_json_delta(parsed.index, partial_json);
                    },
                }
            },
            "content_block_start" => {
                // 处理新的内容块
            },
            "message_delta" => {
                // 处理消息级别的更新（stop_reason, usage 等）
            },
            _ => {},
        }
        
        Ok(())
    }
}
```

---

## 四、存储系统设计

### 4.1 双层存储架构

```
┌─────────────────────────────────────────────────────┐
│                    查询层                           │
│  (按 ID / 时间 / 模型 / Provider / 内容 查询)        │
└─────────────────────────────────────────────────────┘
                         │
         ┌───────────────┼───────────────┐
         ▼               ▼               ▼
┌─────────────┐   ┌─────────────┐   ┌─────────────┐
│  内存缓存   │   │   索引层    │   │   文件层    │
│ (热数据)    │   │  (SQLite)   │   │  (JSONL)    │
│ 最近 1000   │   │ 元数据索引  │   │ 完整数据    │
└─────────────┘   └─────────────┘   └─────────────┘
```

### 4.2 内存缓存

```rust
/// 内存 Flow 存储
pub struct FlowMemoryStore {
    /// 按 ID 索引的 flows
    flows: HashMap<String, Arc<RwLock<LLMFlow>>>,
    /// 按时间排序的 flow IDs
    ordered_ids: VecDeque<String>,
    /// 最大缓存数量
    max_size: usize,
    /// 内存使用估算
    memory_usage: AtomicUsize,
}

impl FlowMemoryStore {
    /// 添加 flow
    pub fn add(&mut self, flow: LLMFlow) {
        let id = flow.id.clone();
        let size = self.estimate_size(&flow);
        
        self.flows.insert(id.clone(), Arc::new(RwLock::new(flow)));
        self.ordered_ids.push_back(id);
        self.memory_usage.fetch_add(size, Ordering::Relaxed);
        
        // 驱逐旧数据
        while self.ordered_ids.len() > self.max_size {
            if let Some(old_id) = self.ordered_ids.pop_front() {
                if let Some(old_flow) = self.flows.remove(&old_id) {
                    let old_size = self.estimate_size(&old_flow.read());
                    self.memory_usage.fetch_sub(old_size, Ordering::Relaxed);
                }
            }
        }
    }
    
    /// 获取最近 N 条
    pub fn get_recent(&self, limit: usize) -> Vec<Arc<RwLock<LLMFlow>>> {
        self.ordered_ids
            .iter()
            .rev()
            .take(limit)
            .filter_map(|id| self.flows.get(id).cloned())
            .collect()
    }
}
```

### 4.3 文件持久化

```rust
/// Flow 文件存储
pub struct FlowFileStore {
    /// 存储目录
    base_dir: PathBuf,
    /// 当前写入文件
    current_file: RwLock<Option<FlowWriter>>,
    /// 轮转配置
    rotation_config: RotationConfig,
}

/// 轮转配置
pub struct RotationConfig {
    /// 按日期轮转
    pub rotate_daily: bool,
    /// 单文件最大大小
    pub max_file_size: u64,
    /// 保留天数
    pub retention_days: u32,
    /// 是否压缩旧文件
    pub compress_old: bool,
}

impl FlowFileStore {
    /// 存储文件结构：
    /// ~/.proxycast/flows/
    /// ├── 2024-01-15/
    /// │   ├── flows_001.jsonl
    /// │   ├── flows_002.jsonl
    /// │   └── index.sqlite  (当日索引)
    /// ├── 2024-01-14/
    /// │   ├── flows.jsonl.gz  (压缩后)
    /// │   └── index.sqlite
    /// └── global_index.sqlite  (全局索引)
    
    /// 写入 flow
    pub async fn write(&self, flow: &LLMFlow) -> Result<(), Error> {
        let mut writer = self.get_or_create_writer().await?;
        
        // 写入 JSONL
        let json = serde_json::to_string(flow)?;
        writer.write_line(&json).await?;
        
        // 更新索引
        self.update_index(flow).await?;
        
        // 检查是否需要轮转
        if writer.size() > self.rotation_config.max_file_size {
            self.rotate().await?;
        }
        
        Ok(())
    }
    
    /// 按条件查询
    pub async fn query(&self, filter: &FlowFilter) -> Result<Vec<LLMFlow>, Error> {
        // 先查询索引获取文件位置
        let locations = self.query_index(filter).await?;
        
        // 从文件读取
        let mut flows = Vec::new();
        for loc in locations {
            let flow = self.read_flow(&loc).await?;
            if filter.matches(&flow) {
                flows.push(flow);
            }
        }
        
        Ok(flows)
    }
}
```

### 4.4 SQLite 索引

```sql
-- 全局索引表
CREATE TABLE flow_index (
    id TEXT PRIMARY KEY,
    created_at DATETIME NOT NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    status TEXT NOT NULL,
    duration_ms INTEGER,
    input_tokens INTEGER,
    output_tokens INTEGER,
    has_error BOOLEAN DEFAULT FALSE,
    has_tool_calls BOOLEAN DEFAULT FALSE,
    has_thinking BOOLEAN DEFAULT FALSE,
    file_path TEXT NOT NULL,
    file_offset INTEGER NOT NULL,
    -- 用于全文搜索
    content_preview TEXT,
    request_preview TEXT
);

CREATE INDEX idx_created_at ON flow_index(created_at);
CREATE INDEX idx_provider ON flow_index(provider);
CREATE INDEX idx_model ON flow_index(model);
CREATE INDEX idx_status ON flow_index(status);

-- 全文搜索表（可选，使用 FTS5）
CREATE VIRTUAL TABLE flow_fts USING fts5(
    id,
    content,
    request,
    thinking,
    content='flow_index'
);
```

---

## 五、查询与过滤

### 5.1 过滤器设计

```rust
/// Flow 过滤器
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowFilter {
    /// 时间范围
    pub time_range: Option<TimeRange>,
    
    /// Provider 过滤
    pub providers: Option<Vec<ProviderType>>,
    
    /// 模型过滤（支持通配符）
    pub models: Option<Vec<String>>,
    
    /// 状态过滤
    pub states: Option<Vec<FlowState>>,
    
    /// 是否有错误
    pub has_error: Option<bool>,
    
    /// 是否有工具调用
    pub has_tool_calls: Option<bool>,
    
    /// 是否有思维链
    pub has_thinking: Option<bool>,
    
    /// 是否流式
    pub is_streaming: Option<bool>,
    
    /// 内容搜索（全文）
    pub content_search: Option<String>,
    
    /// 请求内容搜索
    pub request_search: Option<String>,
    
    /// Token 范围
    pub token_range: Option<TokenRange>,
    
    /// 延迟范围
    pub latency_range: Option<LatencyRange>,
    
    /// 标签过滤
    pub tags: Option<Vec<String>>,
    
    /// 只显示收藏
    pub starred_only: bool,
    
    /// 凭证 ID
    pub credential_id: Option<String>,
}

/// 排序选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FlowSortBy {
    /// 创建时间（默认）
    CreatedAt,
    /// 耗时
    Duration,
    /// Token 数
    TotalTokens,
    /// 内容长度
    ContentLength,
    /// 模型
    Model,
}
```

### 5.2 查询 API

```rust
/// Flow 查询服务
pub struct FlowQueryService {
    memory_store: Arc<FlowMemoryStore>,
    file_store: Arc<FlowFileStore>,
}

impl FlowQueryService {
    /// 查询 flows
    pub async fn query(&self, 
        filter: FlowFilter, 
        sort_by: FlowSortBy,
        sort_desc: bool,
        page: usize,
        page_size: usize,
    ) -> Result<FlowQueryResult, Error> {
        // 优先从内存查询
        let mut flows = self.memory_store.query(&filter);
        
        // 如果需要更多数据，从文件查询
        if flows.len() < page * page_size {
            let file_flows = self.file_store.query(&filter).await?;
            flows.extend(file_flows);
        }
        
        // 排序
        self.sort_flows(&mut flows, sort_by, sort_desc);
        
        // 分页
        let total = flows.len();
        let start = page * page_size;
        let end = (start + page_size).min(total);
        let flows = flows[start..end].to_vec();
        
        Ok(FlowQueryResult {
            flows,
            total,
            page,
            page_size,
        })
    }
    
    /// 获取统计信息
    pub async fn get_stats(&self, filter: &FlowFilter) -> FlowStats {
        // 计算聚合统计
    }
    
    /// 全文搜索
    pub async fn search(&self, query: &str, limit: usize) -> Vec<FlowSearchResult> {
        // 使用 FTS 搜索
    }
}
```

---

## 六、导出功能

### 6.1 支持的导出格式

```rust
/// 导出格式
pub enum ExportFormat {
    /// HAR (HTTP Archive) 格式
    HAR,
    /// JSON 格式
    JSON,
    /// JSONL (每行一个 JSON)
    JSONL,
    /// Markdown 格式（用于文档）
    Markdown,
    /// CSV 格式（仅元数据）
    CSV,
    /// OpenAI JSONL（用于 fine-tuning）
    OpenAIFineTune,
    /// Anthropic JSONL（用于 fine-tuning）
    AnthropicFineTune,
}

/// 导出选项
pub struct ExportOptions {
    /// 导出格式
    pub format: ExportFormat,
    /// 过滤器
    pub filter: FlowFilter,
    /// 是否包含原始数据
    pub include_raw: bool,
    /// 是否包含流式 chunks
    pub include_stream_chunks: bool,
    /// 是否脱敏
    pub redact_sensitive: bool,
    /// 脱敏规则
    pub redaction_rules: Vec<RedactionRule>,
    /// 是否压缩
    pub compress: bool,
}
```

### 6.2 HAR 导出

```rust
impl FlowExporter {
    /// 导出为 HAR 格式
    pub fn export_har(&self, flows: &[LLMFlow]) -> HarArchive {
        HarArchive {
            log: HarLog {
                version: "1.2".to_string(),
                creator: HarCreator {
                    name: "ProxyCast".to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                },
                entries: flows.iter().map(|f| self.flow_to_har_entry(f)).collect(),
                // LLM 特定扩展
                _llm_metadata: Some(LLMHarMetadata {
                    total_tokens: flows.iter().map(|f| f.response.as_ref().map(|r| r.usage.total_tokens).unwrap_or(0) as u64).sum(),
                    models_used: flows.iter().map(|f| f.request.model.clone()).collect::<HashSet<_>>().into_iter().collect(),
                    providers_used: flows.iter().map(|f| f.metadata.provider.to_string()).collect::<HashSet<_>>().into_iter().collect(),
                }),
            },
        }
    }
    
    fn flow_to_har_entry(&self, flow: &LLMFlow) -> HarEntry {
        HarEntry {
            started_date_time: flow.timestamps.created.to_rfc3339(),
            time: flow.timestamps.duration_ms as f64,
            request: HarRequest {
                method: flow.request.method.clone(),
                url: format!("https://api.provider.com{}", flow.request.path),
                http_version: "HTTP/1.1".to_string(),
                headers: flow.request.headers.iter()
                    .map(|(k, v)| HarHeader { name: k.clone(), value: v.clone() })
                    .collect(),
                post_data: Some(HarPostData {
                    mime_type: "application/json".to_string(),
                    text: serde_json::to_string(&flow.request.body).unwrap(),
                }),
                // ...
            },
            response: flow.response.as_ref().map(|r| HarResponse {
                status: r.status_code as i32,
                status_text: r.status_text.clone(),
                headers: r.headers.iter()
                    .map(|(k, v)| HarHeader { name: k.clone(), value: v.clone() })
                    .collect(),
                content: HarContent {
                    size: r.size_bytes as i64,
                    mime_type: "application/json".to_string(),
                    text: Some(serde_json::to_string(&r.body).unwrap()),
                },
                // ...
            }),
            // LLM 特定扩展
            _llm: Some(LLMHarExtension {
                provider: flow.metadata.provider.to_string(),
                model: flow.request.model.clone(),
                input_tokens: flow.response.as_ref().map(|r| r.usage.input_tokens),
                output_tokens: flow.response.as_ref().map(|r| r.usage.output_tokens),
                has_tool_calls: flow.response.as_ref().map(|r| !r.tool_calls.is_empty()).unwrap_or(false),
                has_thinking: flow.response.as_ref().and_then(|r| r.thinking.as_ref()).is_some(),
            }),
        }
    }
}
```

### 6.3 Markdown 导出（用于文档和分享）

```rust
impl FlowExporter {
    /// 导出为 Markdown（用于复制分享）
    pub fn export_markdown(&self, flow: &LLMFlow) -> String {
        let mut md = String::new();
        
        // 标题
        writeln!(md, "# LLM Request - {}", flow.id).unwrap();
        writeln!(md, "").unwrap();
        
        // 元信息
        writeln!(md, "## Metadata").unwrap();
        writeln!(md, "- **Provider**: {}", flow.metadata.provider).unwrap();
        writeln!(md, "- **Model**: {}", flow.request.model).unwrap();
        writeln!(md, "- **Time**: {}", flow.timestamps.created).unwrap();
        writeln!(md, "- **Duration**: {}ms", flow.timestamps.duration_ms).unwrap();
        writeln!(md, "").unwrap();
        
        // 请求
        writeln!(md, "## Request").unwrap();
        if let Some(system) = &flow.request.system_prompt {
            writeln!(md, "### System Prompt").unwrap();
            writeln!(md, "```").unwrap();
            writeln!(md, "{}", system).unwrap();
            writeln!(md, "```").unwrap();
        }
        
        writeln!(md, "### Messages").unwrap();
        for msg in &flow.request.messages {
            writeln!(md, "**{}**:", msg.role).unwrap();
            writeln!(md, "{}", msg.content.to_string()).unwrap();
            writeln!(md, "").unwrap();
        }
        
        // 响应
        if let Some(resp) = &flow.response {
            writeln!(md, "## Response").unwrap();
            
            if let Some(thinking) = &resp.thinking {
                writeln!(md, "### Thinking").unwrap();
                writeln!(md, "<details><summary>Click to expand</summary>").unwrap();
                writeln!(md, "").unwrap();
                writeln!(md, "{}", thinking.text).unwrap();
                writeln!(md, "</details>").unwrap();
                writeln!(md, "").unwrap();
            }
            
            writeln!(md, "### Content").unwrap();
            writeln!(md, "{}", resp.content).unwrap();
            
            if !resp.tool_calls.is_empty() {
                writeln!(md, "### Tool Calls").unwrap();
                for tc in &resp.tool_calls {
                    writeln!(md, "- **{}**: `{}`", tc.function.name, tc.function.arguments).unwrap();
                }
            }
            
            writeln!(md, "### Usage").unwrap();
            writeln!(md, "- Input: {} tokens", resp.usage.input_tokens).unwrap();
            writeln!(md, "- Output: {} tokens", resp.usage.output_tokens).unwrap();
        }
        
        md
    }
}
```

---

## 七、前端界面设计

### 7.1 流量列表视图

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ 🔍 Search...  │ Provider ▾ │ Model ▾ │ Status ▾ │ Time Range ▾ │ ⚙️ Export │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ ┌─────────────────────────────────────────────────────────────────────────┐ │
│ │ ⭐ 14:32:05 │ claude-sonnet-4-5 │ Kiro │ ✅ 2.3s │ 1.2k→3.4k │ 🔧 tool │ │
│ │    "请帮我分析这段代码的性能问题..."                                    │ │
│ └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                             │
│ ┌─────────────────────────────────────────────────────────────────────────┐ │
│ │    14:31:42 │ gemini-2.5-flash │ Gemini │ ✅ 0.8s │ 500→1.2k │         │ │
│ │    "Write a Python function to..."                                      │ │
│ └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                             │
│ ┌─────────────────────────────────────────────────────────────────────────┐ │
│ │    14:31:15 │ claude-sonnet-4-5 │ Kiro │ ❌ 5.2s │ Error: Rate limit   │ │
│ │    "Explain the difference between..."                                  │ │
│ └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 7.2 流量详情视图

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ ← Back │ Request abc123 │ ⭐ Star │ 📋 Copy │ 📤 Export │ 🔄 Replay        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ ┌─ Metadata ────────────────────────────────────────────────────────────┐  │
│ │ Provider: Kiro          Model: claude-sonnet-4-5                      │  │
│ │ Duration: 2.3s          TTFB: 1.2s                                    │  │
│ │ Tokens: 1,234 → 3,456   Cost: $0.045                                  │  │
│ │ Credential: work-account-1                                             │  │
│ └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│ ┌─ Request ─────────────────────────────────────────────────────────────┐  │
│ │ [Headers] [Body] [Messages] [Tools]                                   │  │
│ │                                                                       │  │
│ │ System: You are a helpful assistant...                                │  │
│ │                                                                       │  │
│ │ User: 请帮我分析这段代码的性能问题：                                    │  │
│ │ ```python                                                             │  │
│ │ def slow_function():                                                  │  │
│ │     for i in range(10000):                                            │  │
│ │         result = expensive_operation(i)                               │  │
│ │ ```                                                                   │  │
│ └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│ ┌─ Response ────────────────────────────────────────────────────────────┐  │
│ │ [Content] [Thinking] [Tool Calls] [Raw] [Stream]                      │  │
│ │                                                                       │  │
│ │ 这段代码存在几个性能问题：                                              │  │
│ │                                                                       │  │
│ │ 1. **循环中的重复计算**：`expensive_operation` 被调用 10000 次...      │  │
│ │ 2. **缺少缓存**：如果操作结果可以重用...                                │  │
│ │                                                                       │  │
│ │ [Show more...]                                                        │  │
│ └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│ ┌─ Timeline ────────────────────────────────────────────────────────────┐  │
│ │ Request ████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0.1s     │  │
│ │ TTFB    ░░░░████████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 1.2s     │  │
│ │ Stream  ░░░░░░░░░░░░░░░░░░░░░░░░██████████████████████████░ 1.0s     │  │
│ │ Total   ████████████████████████████████████████████████████ 2.3s     │  │
│ └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 7.3 统计仪表板

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         📊 Flow Statistics                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ ┌─ Overview ──────────────────────────┐ ┌─ Token Usage ─────────────────┐  │
│ │ Total Requests    │ 1,234           │ │                               │  │
│ │ Success Rate      │ 98.2%           │ │ ███████████ Input: 1.2M       │  │
│ │ Avg Latency       │ 1.8s            │ │ █████████████████ Output: 2.1M│  │
│ │ Total Tokens      │ 3.3M            │ │                               │  │
│ └─────────────────────────────────────┘ └───────────────────────────────┘  │
│                                                                             │
│ ┌─ Requests by Provider ──────────────────────────────────────────────────┐│
│ │ Kiro     ██████████████████████████████████████████░░░░░░░░ 68%        ││
│ │ Gemini   ████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 22%        ││
│ │ OpenAI   ████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 10%        ││
│ └─────────────────────────────────────────────────────────────────────────┘│
│                                                                             │
│ ┌─ Latency Distribution ─────────────┐ ┌─ Requests Timeline ───────────┐  │
│ │     ▃▅█▇▅▃▂▁                       │ │ ▂▃▅▇█▇▅▃▂▁▂▃▅▇█▇▅▃▂          │  │
│ │ 0s  1s  2s  3s  4s  5s+            │ │ 00:00    06:00    12:00   18:00│  │
│ └────────────────────────────────────┘ └────────────────────────────────┘  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 八、API 设计

### 8.1 Tauri Commands

```rust
// 查询 flows
#[tauri::command]
async fn query_flows(
    filter: FlowFilter,
    sort_by: Option<FlowSortBy>,
    sort_desc: Option<bool>,
    page: Option<usize>,
    page_size: Option<usize>,
    state: State<'_, FlowMonitorState>,
) -> Result<FlowQueryResult, String>;

// 获取单个 flow 详情
#[tauri::command]
async fn get_flow_detail(
    id: String,
    state: State<'_, FlowMonitorState>,
) -> Result<LLMFlow, String>;

// 搜索 flows
#[tauri::command]
async fn search_flows(
    query: String,
    limit: Option<usize>,
    state: State<'_, FlowMonitorState>,
) -> Result<Vec<FlowSearchResult>, String>;

// 获取统计信息
#[tauri::command]
async fn get_flow_stats(
    filter: Option<FlowFilter>,
    state: State<'_, FlowMonitorState>,
) -> Result<FlowStats, String>;

// 导出 flows
#[tauri::command]
async fn export_flows(
    options: ExportOptions,
    path: String,
    state: State<'_, FlowMonitorState>,
) -> Result<ExportResult, String>;

// 更新 flow 标注
#[tauri::command]
async fn update_flow_annotations(
    id: String,
    annotations: FlowAnnotations,
    state: State<'_, FlowMonitorState>,
) -> Result<(), String>;

// 重放请求
#[tauri::command]
async fn replay_flow(
    id: String,
    modifications: Option<FlowModifications>,
    state: State<'_, FlowMonitorState>,
) -> Result<LLMFlow, String>;

// 清理旧数据
#[tauri::command]
async fn cleanup_flows(
    before: DateTime<Utc>,
    state: State<'_, FlowMonitorState>,
) -> Result<CleanupResult, String>;
```

### 8.2 WebSocket 实时推送

```rust
/// 实时 Flow 事件
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum FlowEvent {
    /// 新 flow 开始
    FlowStarted { flow: FlowSummary },
    /// flow 更新（收到响应数据）
    FlowUpdated { id: String, update: FlowUpdate },
    /// flow 完成
    FlowCompleted { id: String, summary: FlowSummary },
    /// flow 失败
    FlowFailed { id: String, error: FlowError },
    /// 统计更新
    StatsUpdated { stats: FlowStats },
}

/// Flow 摘要（用于列表显示）
#[derive(Debug, Clone, Serialize)]
pub struct FlowSummary {
    pub id: String,
    pub provider: String,
    pub model: String,
    pub state: FlowState,
    pub duration_ms: Option<u64>,
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub content_preview: String,
    pub has_error: bool,
    pub has_tool_calls: bool,
    pub created_at: DateTime<Utc>,
}
```

---

## 九、性能与隐私

### 9.1 性能优化

```rust
/// Flow 监控配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowMonitorConfig {
    /// 是否启用监控
    pub enabled: bool,
    
    /// 内存中最大 flow 数量
    pub max_memory_flows: usize,
    
    /// 是否保存到文件
    pub persist_to_file: bool,
    
    /// 文件保留天数
    pub retention_days: u32,
    
    /// 是否保存原始 stream chunks
    pub save_stream_chunks: bool,
    
    /// 请求体大小限制（超过则截断）
    pub max_request_body_size: usize,
    
    /// 响应体大小限制
    pub max_response_body_size: usize,
    
    /// 是否保存图片内容（base64）
    pub save_image_content: bool,
    
    /// 图片缩略图大小
    pub thumbnail_size: (u32, u32),
    
    /// 采样率（0.0-1.0，用于高流量场景）
    pub sampling_rate: f32,
    
    /// 排除的模型（不记录）
    pub excluded_models: Vec<String>,
    
    /// 排除的路径
    pub excluded_paths: Vec<String>,
}

impl Default for FlowMonitorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_memory_flows: 1000,
            persist_to_file: true,
            retention_days: 7,
            save_stream_chunks: false, // 默认不保存原始 chunks
            max_request_body_size: 1024 * 1024, // 1MB
            max_response_body_size: 10 * 1024 * 1024, // 10MB
            save_image_content: false, // 默认不保存图片
            thumbnail_size: (100, 100),
            sampling_rate: 1.0,
            excluded_models: vec![],
            excluded_paths: vec!["/health".to_string()],
        }
    }
}
```

### 9.2 隐私保护

```rust
/// 脱敏规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionRule {
    /// 规则名称
    pub name: String,
    /// 匹配模式（正则）
    pub pattern: String,
    /// 替换内容
    pub replacement: String,
    /// 应用位置
    pub apply_to: Vec<RedactionTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RedactionTarget {
    /// 请求头
    RequestHeaders,
    /// 请求体
    RequestBody,
    /// 响应头
    ResponseHeaders,
    /// 响应体
    ResponseBody,
    /// 所有位置
    All,
}

impl Default for Vec<RedactionRule> {
    fn default() -> Self {
        vec![
            // API Key 脱敏
            RedactionRule {
                name: "api_key".to_string(),
                pattern: r"(sk-[a-zA-Z0-9]{20,}|api[_-]?key[=:]\s*['\"]?)[a-zA-Z0-9\-_]+".to_string(),
                replacement: "$1***REDACTED***".to_string(),
                apply_to: vec![RedactionTarget::All],
            },
            // Email 脱敏
            RedactionRule {
                name: "email".to_string(),
                pattern: r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}".to_string(),
                replacement: "***@***.***".to_string(),
                apply_to: vec![RedactionTarget::RequestBody, RedactionTarget::ResponseBody],
            },
            // 手机号脱敏
            RedactionRule {
                name: "phone".to_string(),
                pattern: r"\b1[3-9]\d{9}\b".to_string(),
                replacement: "1**********".to_string(),
                apply_to: vec![RedactionTarget::RequestBody, RedactionTarget::ResponseBody],
            },
        ]
    }
}
```

---

## 十、实现路线图

### Phase 1: 基础设施（1-2 周）

- [ ] 定义完整的数据模型（LLMFlow, LLMRequest, LLMResponse）
- [ ] 实现内存存储 FlowMemoryStore
- [ ] 实现 SSE 流重建器 StreamRebuilder
- [ ] 在现有 API handlers 中集成 flow 捕获

### Phase 2: 持久化与查询（1-2 周）

- [ ] 实现文件存储 FlowFileStore
- [ ] 实现 SQLite 索引
- [ ] 实现查询过滤器
- [ ] 添加全文搜索支持

### Phase 3: 前端界面（2-3 周）

- [ ] 实现 Flow 列表页面
- [ ] 实现 Flow 详情页面
- [ ] 实现统计仪表板
- [ ] 实现实时更新（WebSocket）

### Phase 4: 导出与高级功能（1-2 周）

- [ ] 实现 HAR 导出
- [ ] 实现 Markdown 导出
- [ ] 实现请求重放
- [ ] 实现隐私脱敏

### Phase 5: 优化与文档（1 周）

- [ ] 性能优化
- [ ] 编写用户文档
- [ ] 添加测试用例
- [ ] 发布 v1.0

---

## 十一、附录

### A. 与现有系统的集成点

1. **server/handlers/api.rs**: 在 `chat_completions` 和 `anthropic_messages` 函数中添加 flow 捕获
2. **server_utils.rs**: 复用 `parse_cw_response` 用于流式响应解析
3. **services/provider_pool_service.rs**: 获取凭证信息用于 metadata
4. **models/log_model.rs**: 将 RequestLog 与 LLMFlow 关联

### B. 参考实现

- [mitmproxy](https://github.com/mitmproxy/mitmproxy) - HTTP 流量捕获的黄金标准
- [Charles Proxy](https://www.charlesproxy.com/) - 商业代理调试工具
- [Fiddler](https://www.telerik.com/fiddler) - .NET 平台代理调试工具
- [LangSmith](https://smith.langchain.com/) - LangChain 官方的 LLM 可观测性平台

### C. 数据大小估算

| 场景 | 请求数/天 | 平均大小 | 日存储量 | 月存储量 |
|------|----------|---------|---------|---------|
| 个人开发 | 100 | 10KB | 1MB | 30MB |
| 团队开发 | 1,000 | 15KB | 15MB | 450MB |
| 生产环境 | 10,000 | 20KB | 200MB | 6GB |

### D. 安全考虑

1. **本地存储**：所有数据存储在本地，不上传到任何服务器
2. **访问控制**：通过 API Key 验证访问
3. **数据加密**：敏感数据可选加密存储
4. **审计日志**：记录所有导出和访问操作

---

## 十二、开放问题

1. **图片处理策略**：是否保存完整的 base64 图片内容？还是只保存缩略图？
2. **音频处理**：如何处理音频内容？
3. **多租户支持**：是否需要支持多个 workspace 隔离数据？
4. **云同步**：是否需要支持跨设备同步 flow 数据？
5. **对比功能**：是否需要支持两个 flow 的对比功能？
6. **回归测试**：是否需要将保存的 flow 作为回归测试用例？

---

*文档版本：v1.0*
*最后更新：2024-01*
*作者：ProxyCast Team*
