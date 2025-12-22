//! 流式传输错误类型
//!
//! 定义流式传输过程中可能发生的各种错误类型。
//!
//! # 需求覆盖
//!
//! - 需求 6.1: 网络错误处理
//! - 需求 6.2: 超时错误处理
//! - 需求 6.3: Provider 错误转发

use serde::{Deserialize, Serialize};
use std::fmt;

/// 流式传输错误类型
///
/// 涵盖流式传输过程中可能发生的所有错误情况。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "details")]
pub enum StreamError {
    /// 网络错误
    ///
    /// 当网络连接失败、DNS 解析失败或连接被重置时发生。
    /// 对应需求 6.1
    Network(String),

    /// 超时错误
    ///
    /// 当流式响应超过配置的超时时间时发生。
    /// 对应需求 6.2
    Timeout,

    /// 解析错误
    ///
    /// 当无法解析流式数据（如无效的 AWS Event Stream 或 SSE 格式）时发生。
    ParseError(String),

    /// Provider 错误
    ///
    /// 当上游 Provider 返回错误响应时发生。
    /// 对应需求 6.3
    ProviderError {
        /// HTTP 状态码
        status: u16,
        /// 错误消息
        message: String,
    },

    /// 客户端断开连接
    ///
    /// 当客户端在流式传输过程中断开连接时发生。
    ClientDisconnected,

    /// 缓冲区溢出
    ///
    /// 当流式数据超过配置的缓冲区大小时发生。
    BufferOverflow,

    /// 内部错误
    ///
    /// 其他内部错误。
    Internal(String),
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StreamError::Network(msg) => write!(f, "网络错误: {}", msg),
            StreamError::Timeout => write!(f, "流式响应超时"),
            StreamError::ParseError(msg) => write!(f, "解析错误: {}", msg),
            StreamError::ProviderError { status, message } => {
                write!(f, "Provider 错误 ({}): {}", status, message)
            }
            StreamError::ClientDisconnected => write!(f, "客户端已断开连接"),
            StreamError::BufferOverflow => write!(f, "缓冲区溢出"),
            StreamError::Internal(msg) => write!(f, "内部错误: {}", msg),
        }
    }
}

impl std::error::Error for StreamError {}

// ============================================================================
// From trait 实现 - 用于错误转换
// ============================================================================

impl From<std::io::Error> for StreamError {
    fn from(err: std::io::Error) -> Self {
        StreamError::Network(err.to_string())
    }
}

impl From<reqwest::Error> for StreamError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            StreamError::Timeout
        } else if err.is_connect() {
            StreamError::Network(format!("连接失败: {}", err))
        } else if err.is_request() {
            StreamError::Network(format!("请求错误: {}", err))
        } else {
            StreamError::Network(err.to_string())
        }
    }
}

impl From<serde_json::Error> for StreamError {
    fn from(err: serde_json::Error) -> Self {
        StreamError::ParseError(err.to_string())
    }
}

impl From<String> for StreamError {
    fn from(msg: String) -> Self {
        StreamError::Internal(msg)
    }
}

impl From<&str> for StreamError {
    fn from(msg: &str) -> Self {
        StreamError::Internal(msg.to_string())
    }
}

// ============================================================================
// 辅助方法
// ============================================================================

impl StreamError {
    /// 创建网络错误
    pub fn network(msg: impl Into<String>) -> Self {
        StreamError::Network(msg.into())
    }

    /// 创建解析错误
    pub fn parse_error(msg: impl Into<String>) -> Self {
        StreamError::ParseError(msg.into())
    }

    /// 创建 Provider 错误
    pub fn provider_error(status: u16, message: impl Into<String>) -> Self {
        StreamError::ProviderError {
            status,
            message: message.into(),
        }
    }

    /// 创建内部错误
    pub fn internal(msg: impl Into<String>) -> Self {
        StreamError::Internal(msg.into())
    }

    /// 判断错误是否可重试
    ///
    /// 网络错误、超时和某些 Provider 错误（如 429、5xx）可以重试。
    pub fn is_retryable(&self) -> bool {
        match self {
            StreamError::Network(_) => true,
            StreamError::Timeout => true,
            StreamError::ProviderError { status, .. } => *status == 429 || *status >= 500,
            StreamError::ParseError(_) => false,
            StreamError::ClientDisconnected => false,
            StreamError::BufferOverflow => false,
            StreamError::Internal(_) => false,
        }
    }

    /// 判断是否为客户端错误
    pub fn is_client_error(&self) -> bool {
        matches!(self, StreamError::ClientDisconnected)
    }

    /// 获取 HTTP 状态码（如果适用）
    pub fn status_code(&self) -> Option<u16> {
        match self {
            StreamError::ProviderError { status, .. } => Some(*status),
            StreamError::Timeout => Some(504), // Gateway Timeout
            StreamError::Network(_) => Some(502), // Bad Gateway
            _ => None,
        }
    }

    /// 转换为 SSE 错误事件格式
    pub fn to_sse_error(&self) -> String {
        let error_json = serde_json::json!({
            "error": {
                "type": self.error_type_string(),
                "message": self.to_string(),
            }
        });
        format!("event: error\ndata: {}\n\n", error_json)
    }

    /// 获取错误类型字符串
    fn error_type_string(&self) -> &'static str {
        match self {
            StreamError::Network(_) => "network_error",
            StreamError::Timeout => "timeout",
            StreamError::ParseError(_) => "parse_error",
            StreamError::ProviderError { .. } => "provider_error",
            StreamError::ClientDisconnected => "client_disconnected",
            StreamError::BufferOverflow => "buffer_overflow",
            StreamError::Internal(_) => "internal_error",
        }
    }
}

// ============================================================================
// 测试模块
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_error_display() {
        let err = StreamError::Network("connection refused".to_string());
        assert_eq!(err.to_string(), "网络错误: connection refused");

        let err = StreamError::Timeout;
        assert_eq!(err.to_string(), "流式响应超时");

        let err = StreamError::provider_error(429, "rate limited");
        assert_eq!(err.to_string(), "Provider 错误 (429): rate limited");
    }

    #[test]
    fn test_stream_error_is_retryable() {
        assert!(StreamError::Network("test".to_string()).is_retryable());
        assert!(StreamError::Timeout.is_retryable());
        assert!(StreamError::provider_error(429, "rate limited").is_retryable());
        assert!(StreamError::provider_error(500, "server error").is_retryable());
        assert!(!StreamError::provider_error(400, "bad request").is_retryable());
        assert!(!StreamError::ParseError("invalid json".to_string()).is_retryable());
        assert!(!StreamError::ClientDisconnected.is_retryable());
    }

    #[test]
    fn test_stream_error_status_code() {
        assert_eq!(StreamError::Timeout.status_code(), Some(504));
        assert_eq!(
            StreamError::Network("test".to_string()).status_code(),
            Some(502)
        );
        assert_eq!(
            StreamError::provider_error(429, "test").status_code(),
            Some(429)
        );
        assert_eq!(StreamError::ClientDisconnected.status_code(), None);
    }

    #[test]
    fn test_stream_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        let stream_err: StreamError = io_err.into();
        assert!(matches!(stream_err, StreamError::Network(_)));
    }

    #[test]
    fn test_stream_error_from_serde_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let stream_err: StreamError = json_err.into();
        assert!(matches!(stream_err, StreamError::ParseError(_)));
    }

    #[test]
    fn test_stream_error_serialization() {
        let err = StreamError::provider_error(500, "internal server error");
        let json = serde_json::to_string(&err).unwrap();
        let deserialized: StreamError = serde_json::from_str(&json).unwrap();
        assert_eq!(err, deserialized);
    }

    #[test]
    fn test_stream_error_to_sse_error() {
        let err = StreamError::Timeout;
        let sse = err.to_sse_error();
        assert!(sse.starts_with("event: error\n"));
        assert!(sse.contains("timeout"));
    }
}
