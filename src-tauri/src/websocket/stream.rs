//! WebSocket 流式响应处理
//!
//! 将 SSE 流转换为 WebSocket 消息，实现背压控制

use super::{MessageProcessor, WsError, WsMessage};
use futures::{Stream, StreamExt};
use tokio::sync::mpsc;

/// 流式响应转发器
pub struct StreamForwarder {
    /// 请求 ID
    request_id: String,
    /// 背压缓冲区大小
    buffer_size: usize,
}

impl StreamForwarder {
    /// 创建新的流式转发器
    pub fn new(request_id: String) -> Self {
        Self {
            request_id,
            buffer_size: 32, // 默认缓冲区大小
        }
    }

    /// 设置缓冲区大小
    pub fn with_buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size;
        self
    }

    /// 将 SSE 数据行转换为 WebSocket 消息
    ///
    /// SSE 格式: "data: {...}\n\n"
    /// 返回: WsStreamChunk 消息
    pub fn convert_sse_line(&self, line: &str, index: u32) -> Option<WsMessage> {
        let trimmed = line.trim();

        // 跳过空行和注释
        if trimmed.is_empty() || trimmed.starts_with(':') {
            return None;
        }

        // 处理 data: 前缀
        let data = if let Some(stripped) = trimmed.strip_prefix("data: ") {
            stripped
        } else if let Some(stripped) = trimmed.strip_prefix("data:") {
            stripped
        } else {
            trimmed
        };

        // 跳过 [DONE] 标记
        if data == "[DONE]" {
            return None;
        }

        Some(MessageProcessor::create_stream_chunk(
            &self.request_id,
            index,
            data,
        ))
    }

    /// 处理完整的 SSE 响应体
    ///
    /// 将 SSE 响应体分割成行并转换为 WebSocket 消息序列
    pub fn process_sse_body(&self, body: &str) -> Vec<WsMessage> {
        let mut messages = Vec::new();
        let mut index = 0u32;

        for line in body.lines() {
            if let Some(msg) = self.convert_sse_line(line, index) {
                messages.push(msg);
                index += 1;
            }
        }

        // 添加结束消息
        messages.push(MessageProcessor::create_stream_end(&self.request_id, index));

        messages
    }

    /// 创建带背压控制的消息通道
    ///
    /// 返回发送端和接收端，用于流式传输
    pub fn create_channel(&self) -> (mpsc::Sender<WsMessage>, mpsc::Receiver<WsMessage>) {
        mpsc::channel(self.buffer_size)
    }

    /// 异步处理 SSE 流
    ///
    /// 从字符串流中读取 SSE 数据并转换为 WebSocket 消息
    pub async fn forward_string_stream<S, E>(
        &self,
        mut stream: S,
        sender: mpsc::Sender<WsMessage>,
    ) -> Result<u32, WsError>
    where
        S: Stream<Item = Result<String, E>> + Unpin,
        E: std::fmt::Display,
    {
        let mut index = 0u32;
        let mut buffer = String::new();

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    buffer.push_str(&chunk);

                    // 处理完整的行
                    while let Some(pos) = buffer.find('\n') {
                        let line = buffer[..pos].to_string();
                        buffer = buffer[pos + 1..].to_string();

                        if let Some(msg) = self.convert_sse_line(&line, index) {
                            // 发送消息，如果通道满则等待（背压）
                            if sender.send(msg).await.is_err() {
                                return Err(WsError::internal(
                                    Some(self.request_id.clone()),
                                    "Channel closed",
                                ));
                            }
                            index += 1;
                        }
                    }
                }
                Err(e) => {
                    return Err(WsError::upstream(
                        Some(self.request_id.clone()),
                        format!("Stream error: {}", e),
                    ));
                }
            }
        }

        // 处理缓冲区中剩余的数据
        if !buffer.is_empty() {
            if let Some(msg) = self.convert_sse_line(&buffer, index) {
                let _ = sender.send(msg).await;
                index += 1;
            }
        }

        // 发送结束消息
        let end_msg = MessageProcessor::create_stream_end(&self.request_id, index);
        let _ = sender.send(end_msg).await;

        Ok(index)
    }
}

/// 背压控制器
pub struct BackpressureController {
    /// 高水位线（暂停发送）
    high_watermark: usize,
    /// 低水位线（恢复发送）
    low_watermark: usize,
    /// 当前队列大小
    current_size: usize,
    /// 是否暂停
    paused: bool,
}

impl BackpressureController {
    /// 创建新的背压控制器
    pub fn new(high_watermark: usize, low_watermark: usize) -> Self {
        Self {
            high_watermark,
            low_watermark,
            current_size: 0,
            paused: false,
        }
    }

    /// 使用默认配置创建
    pub fn with_defaults() -> Self {
        Self::new(64, 16)
    }

    /// 记录消息入队
    pub fn on_enqueue(&mut self) -> bool {
        self.current_size += 1;
        if self.current_size >= self.high_watermark {
            self.paused = true;
        }
        !self.paused
    }

    /// 记录消息出队
    pub fn on_dequeue(&mut self) -> bool {
        if self.current_size > 0 {
            self.current_size -= 1;
        }
        if self.current_size <= self.low_watermark {
            self.paused = false;
        }
        !self.paused
    }

    /// 检查是否应该暂停
    pub fn should_pause(&self) -> bool {
        self.paused
    }

    /// 获取当前队列大小
    pub fn current_size(&self) -> usize {
        self.current_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_sse_line_data() {
        let forwarder = StreamForwarder::new("req-1".to_string());

        let msg = forwarder.convert_sse_line("data: {\"content\": \"hello\"}", 0);
        assert!(msg.is_some());
        match msg.unwrap() {
            WsMessage::StreamChunk(chunk) => {
                assert_eq!(chunk.request_id, "req-1");
                assert_eq!(chunk.index, 0);
                assert_eq!(chunk.data, "{\"content\": \"hello\"}");
            }
            _ => panic!("Expected StreamChunk"),
        }
    }

    #[test]
    fn test_convert_sse_line_empty() {
        let forwarder = StreamForwarder::new("req-1".to_string());
        assert!(forwarder.convert_sse_line("", 0).is_none());
        assert!(forwarder.convert_sse_line("   ", 0).is_none());
    }

    #[test]
    fn test_convert_sse_line_comment() {
        let forwarder = StreamForwarder::new("req-1".to_string());
        assert!(forwarder
            .convert_sse_line(": this is a comment", 0)
            .is_none());
    }

    #[test]
    fn test_convert_sse_line_done() {
        let forwarder = StreamForwarder::new("req-1".to_string());
        assert!(forwarder.convert_sse_line("data: [DONE]", 0).is_none());
    }

    #[test]
    fn test_process_sse_body() {
        let forwarder = StreamForwarder::new("req-1".to_string());
        let body = "data: {\"chunk\": 1}\n\ndata: {\"chunk\": 2}\n\ndata: [DONE]\n\n";

        let messages = forwarder.process_sse_body(body);

        // 应该有 2 个数据块 + 1 个结束消息
        assert_eq!(messages.len(), 3);

        match &messages[0] {
            WsMessage::StreamChunk(c) => assert_eq!(c.index, 0),
            _ => panic!("Expected StreamChunk"),
        }

        match &messages[1] {
            WsMessage::StreamChunk(c) => assert_eq!(c.index, 1),
            _ => panic!("Expected StreamChunk"),
        }

        match &messages[2] {
            WsMessage::StreamEnd(e) => assert_eq!(e.total_chunks, 2),
            _ => panic!("Expected StreamEnd"),
        }
    }

    #[test]
    fn test_backpressure_controller() {
        let mut controller = BackpressureController::new(3, 1);

        // 入队直到高水位
        assert!(controller.on_enqueue()); // 1
        assert!(controller.on_enqueue()); // 2
        assert!(!controller.on_enqueue()); // 3 - 达到高水位，暂停

        assert!(controller.should_pause());

        // 出队直到低水位
        assert!(!controller.on_dequeue()); // 2 - 仍然暂停
        assert!(controller.on_dequeue()); // 1 - 达到低水位，恢复

        assert!(!controller.should_pause());
    }

    #[test]
    fn test_backpressure_controller_defaults() {
        let controller = BackpressureController::with_defaults();
        assert_eq!(controller.current_size(), 0);
        assert!(!controller.should_pause());
    }

    #[test]
    fn test_stream_forwarder_buffer_size() {
        let forwarder = StreamForwarder::new("req-1".to_string()).with_buffer_size(64);
        assert_eq!(forwarder.buffer_size, 64);
    }

    #[test]
    fn test_create_channel() {
        let forwarder = StreamForwarder::new("req-1".to_string()).with_buffer_size(16);
        let (tx, _rx) = forwarder.create_channel();
        // 通道应该被创建成功
        assert!(!tx.is_closed());
    }
}
