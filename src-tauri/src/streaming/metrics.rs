//! 流式传输指标类型
//!
//! 定义流式传输过程中的性能指标和统计数据。
//!
//! # 需求覆盖
//!
//! - 需求 4.5: 跟踪 chunk 数量和接收的总字节数
//! - 需求 7.5: 记录流式指标（吞吐量、延迟、错误率）

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::info;

/// 流式传输指标
///
/// 记录流式传输过程中的各种性能指标。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMetrics {
    /// 首字节时间（毫秒）
    ///
    /// 从请求发送到收到第一个响应字节的时间。
    /// 对应需求 4.1 中的 TTFB 记录。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttfb_ms: Option<u64>,

    /// chunk 数量
    ///
    /// 接收到的流式 chunk 总数。
    /// 对应需求 4.5。
    pub chunk_count: u32,

    /// 总字节数
    ///
    /// 接收到的总字节数。
    /// 对应需求 4.5。
    pub total_bytes: usize,

    /// 开始时间
    ///
    /// 流式传输开始的时间戳。
    pub start_time: DateTime<Utc>,

    /// 结束时间
    ///
    /// 流式传输结束的时间戳（如果已结束）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<DateTime<Utc>>,

    /// 首个 chunk 时间
    ///
    /// 收到第一个 chunk 的时间戳。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_chunk_time: Option<DateTime<Utc>>,

    /// 最后一个 chunk 时间
    ///
    /// 收到最后一个 chunk 的时间戳。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_chunk_time: Option<DateTime<Utc>>,

    /// 解析错误数量
    ///
    /// 流式传输过程中遇到的解析错误数量。
    pub parse_error_count: u32,

    /// 重试次数
    ///
    /// 流式传输过程中的重试次数。
    pub retry_count: u32,

    /// 最小 chunk 大小（字节）
    ///
    /// 对应需求 7.5: 记录流式指标
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_chunk_size: Option<usize>,

    /// 最大 chunk 大小（字节）
    ///
    /// 对应需求 7.5: 记录流式指标
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_chunk_size: Option<usize>,

    /// 缓冲区溢出次数
    ///
    /// 对应需求 7.1: 有界缓冲区
    pub buffer_overflow_count: u32,

    /// 被节流的事件数量
    ///
    /// 对应需求 4.6: 事件节流
    pub throttled_event_count: u32,
}

impl Default for StreamMetrics {
    fn default() -> Self {
        Self {
            ttfb_ms: None,
            chunk_count: 0,
            total_bytes: 0,
            start_time: Utc::now(),
            end_time: None,
            first_chunk_time: None,
            last_chunk_time: None,
            parse_error_count: 0,
            retry_count: 0,
            min_chunk_size: None,
            max_chunk_size: None,
            buffer_overflow_count: 0,
            throttled_event_count: 0,
        }
    }
}

impl StreamMetrics {
    /// 创建新的指标实例
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录收到第一个 chunk
    ///
    /// 自动计算 TTFB 并记录首个 chunk 时间。
    pub fn record_first_chunk(&mut self) {
        let now = Utc::now();
        self.first_chunk_time = Some(now);
        self.ttfb_ms = Some((now - self.start_time).num_milliseconds().max(0) as u64);
    }

    /// 记录收到一个 chunk
    ///
    /// 更新 chunk 计数、字节数和最后 chunk 时间。
    /// 同时更新最小/最大 chunk 大小统计。
    pub fn record_chunk(&mut self, bytes: usize) {
        self.chunk_count += 1;
        self.total_bytes += bytes;
        self.last_chunk_time = Some(Utc::now());

        // 更新最小/最大 chunk 大小（需求 7.5）
        match self.min_chunk_size {
            None => self.min_chunk_size = Some(bytes),
            Some(min) if bytes < min => self.min_chunk_size = Some(bytes),
            _ => {}
        }
        match self.max_chunk_size {
            None => self.max_chunk_size = Some(bytes),
            Some(max) if bytes > max => self.max_chunk_size = Some(bytes),
            _ => {}
        }

        // 如果是第一个 chunk，记录 TTFB
        if self.first_chunk_time.is_none() {
            self.record_first_chunk();
        }
    }

    /// 记录解析错误
    pub fn record_parse_error(&mut self) {
        self.parse_error_count += 1;
    }

    /// 记录重试
    pub fn record_retry(&mut self) {
        self.retry_count += 1;
    }

    /// 记录缓冲区溢出
    ///
    /// 对应需求 7.1: 有界缓冲区
    pub fn record_buffer_overflow(&mut self) {
        self.buffer_overflow_count += 1;
    }

    /// 记录被节流的事件
    ///
    /// 对应需求 4.6: 事件节流
    pub fn record_throttled_event(&mut self) {
        self.throttled_event_count += 1;
    }

    /// 批量记录被节流的事件
    ///
    /// 对应需求 4.6: 事件节流
    pub fn record_throttled_events(&mut self, count: u32) {
        self.throttled_event_count += count;
    }

    /// 完成流式传输
    ///
    /// 记录结束时间。
    pub fn finish(&mut self) {
        self.end_time = Some(Utc::now());
    }

    /// 获取总耗时（毫秒）
    ///
    /// 如果流式传输已结束，返回从开始到结束的时间。
    /// 否则返回从开始到现在的时间。
    pub fn duration_ms(&self) -> u64 {
        let end = self.end_time.unwrap_or_else(Utc::now);
        (end - self.start_time).num_milliseconds().max(0) as u64
    }

    /// 获取平均 chunk 间隔（毫秒）
    ///
    /// 如果 chunk 数量少于 2，返回 None。
    pub fn avg_chunk_interval_ms(&self) -> Option<f64> {
        if self.chunk_count < 2 {
            return None;
        }

        let first = self.first_chunk_time?;
        let last = self.last_chunk_time?;
        let interval_ms = (last - first).num_milliseconds().max(0) as f64;
        Some(interval_ms / (self.chunk_count - 1) as f64)
    }

    /// 获取平均 chunk 大小（字节）
    ///
    /// 对应需求 7.5: 记录流式指标
    pub fn avg_chunk_size(&self) -> Option<f64> {
        if self.chunk_count == 0 {
            return None;
        }
        Some(self.total_bytes as f64 / self.chunk_count as f64)
    }

    /// 获取吞吐量（字节/秒）
    ///
    /// 如果耗时为 0，返回 None。
    pub fn throughput_bytes_per_sec(&self) -> Option<f64> {
        let duration_ms = self.duration_ms();
        if duration_ms == 0 {
            return None;
        }
        Some(self.total_bytes as f64 / (duration_ms as f64 / 1000.0))
    }

    /// 获取错误率
    ///
    /// 解析错误数量 / chunk 数量。
    /// 如果 chunk 数量为 0，返回 0.0。
    pub fn error_rate(&self) -> f64 {
        if self.chunk_count == 0 {
            return 0.0;
        }
        self.parse_error_count as f64 / self.chunk_count as f64
    }

    /// 获取节流率
    ///
    /// 被节流的事件数量 / (被节流的事件数量 + chunk 数量)
    /// 对应需求 4.6: 事件节流
    pub fn throttle_rate(&self) -> f64 {
        let total = self.throttled_event_count + self.chunk_count;
        if total == 0 {
            return 0.0;
        }
        self.throttled_event_count as f64 / total as f64
    }

    /// 判断流式传输是否已完成
    pub fn is_finished(&self) -> bool {
        self.end_time.is_some()
    }

    /// 转换为摘要字符串
    pub fn summary(&self) -> String {
        let duration = self.duration_ms();
        let ttfb = self
            .ttfb_ms
            .map(|t| format!("{}ms", t))
            .unwrap_or_else(|| "N/A".to_string());
        let throughput = self
            .throughput_bytes_per_sec()
            .map(|t| format!("{:.2} KB/s", t / 1024.0))
            .unwrap_or_else(|| "N/A".to_string());
        let avg_chunk = self
            .avg_chunk_size()
            .map(|s| format!("{:.0}B", s))
            .unwrap_or_else(|| "N/A".to_string());

        format!(
            "chunks: {}, bytes: {}, duration: {}ms, ttfb: {}, throughput: {}, avg_chunk: {}, errors: {}, throttled: {}",
            self.chunk_count,
            self.total_bytes,
            duration,
            ttfb,
            throughput,
            avg_chunk,
            self.parse_error_count,
            self.throttled_event_count
        )
    }

    /// 记录详细指标到日志
    ///
    /// 对应需求 7.5: 记录流式指标（吞吐量、延迟、错误率）
    pub fn log_metrics(&self, flow_id: Option<&str>) {
        let throughput = self.throughput_bytes_per_sec().unwrap_or(0.0);
        let error_rate = self.error_rate();
        let throttle_rate = self.throttle_rate();
        let avg_interval = self.avg_chunk_interval_ms().unwrap_or(0.0);

        info!(
            flow_id = ?flow_id,
            chunk_count = self.chunk_count,
            total_bytes = self.total_bytes,
            duration_ms = self.duration_ms(),
            ttfb_ms = ?self.ttfb_ms,
            throughput_kbps = format!("{:.2}", throughput / 1024.0),
            avg_chunk_interval_ms = format!("{:.2}", avg_interval),
            min_chunk_size = ?self.min_chunk_size,
            max_chunk_size = ?self.max_chunk_size,
            avg_chunk_size = ?self.avg_chunk_size(),
            parse_error_count = self.parse_error_count,
            error_rate = format!("{:.4}", error_rate),
            buffer_overflow_count = self.buffer_overflow_count,
            throttled_event_count = self.throttled_event_count,
            throttle_rate = format!("{:.4}", throttle_rate),
            "流式传输指标"
        );
    }
}

// ============================================================================
// 测试模块
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn test_stream_metrics_default() {
        let metrics = StreamMetrics::default();
        assert_eq!(metrics.chunk_count, 0);
        assert_eq!(metrics.total_bytes, 0);
        assert!(metrics.ttfb_ms.is_none());
        assert!(metrics.end_time.is_none());
        assert!(metrics.min_chunk_size.is_none());
        assert!(metrics.max_chunk_size.is_none());
        assert_eq!(metrics.buffer_overflow_count, 0);
        assert_eq!(metrics.throttled_event_count, 0);
    }

    #[test]
    fn test_stream_metrics_record_chunk() {
        let mut metrics = StreamMetrics::new();

        metrics.record_chunk(100);
        assert_eq!(metrics.chunk_count, 1);
        assert_eq!(metrics.total_bytes, 100);
        assert!(metrics.first_chunk_time.is_some());
        assert!(metrics.ttfb_ms.is_some());
        assert_eq!(metrics.min_chunk_size, Some(100));
        assert_eq!(metrics.max_chunk_size, Some(100));

        metrics.record_chunk(200);
        assert_eq!(metrics.chunk_count, 2);
        assert_eq!(metrics.total_bytes, 300);
        assert_eq!(metrics.min_chunk_size, Some(100));
        assert_eq!(metrics.max_chunk_size, Some(200));

        metrics.record_chunk(50);
        assert_eq!(metrics.chunk_count, 3);
        assert_eq!(metrics.total_bytes, 350);
        assert_eq!(metrics.min_chunk_size, Some(50));
        assert_eq!(metrics.max_chunk_size, Some(200));
    }

    #[test]
    fn test_stream_metrics_record_first_chunk() {
        let mut metrics = StreamMetrics::new();

        // 等待一小段时间以确保 TTFB > 0
        sleep(Duration::from_millis(10));

        metrics.record_first_chunk();
        assert!(metrics.first_chunk_time.is_some());
        assert!(metrics.ttfb_ms.is_some());
        assert!(metrics.ttfb_ms.unwrap() >= 10);
    }

    #[test]
    fn test_stream_metrics_finish() {
        let mut metrics = StreamMetrics::new();
        assert!(!metrics.is_finished());

        metrics.finish();
        assert!(metrics.is_finished());
        assert!(metrics.end_time.is_some());
    }

    #[test]
    fn test_stream_metrics_duration() {
        let mut metrics = StreamMetrics::new();

        sleep(Duration::from_millis(50));

        let duration = metrics.duration_ms();
        assert!(duration >= 50);

        metrics.finish();
        let final_duration = metrics.duration_ms();
        assert!(final_duration >= 50);
    }

    #[test]
    fn test_stream_metrics_avg_chunk_interval() {
        let mut metrics = StreamMetrics::new();

        // 单个 chunk 没有间隔
        metrics.record_chunk(100);
        assert!(metrics.avg_chunk_interval_ms().is_none());

        sleep(Duration::from_millis(20));

        metrics.record_chunk(100);
        let interval = metrics.avg_chunk_interval_ms();
        assert!(interval.is_some());
        assert!(interval.unwrap() >= 20.0);
    }

    #[test]
    fn test_stream_metrics_avg_chunk_size() {
        let mut metrics = StreamMetrics::new();

        // 没有 chunk 时没有平均大小
        assert!(metrics.avg_chunk_size().is_none());

        metrics.record_chunk(100);
        assert_eq!(metrics.avg_chunk_size(), Some(100.0));

        metrics.record_chunk(200);
        assert_eq!(metrics.avg_chunk_size(), Some(150.0));

        metrics.record_chunk(300);
        assert_eq!(metrics.avg_chunk_size(), Some(200.0));
    }

    #[test]
    fn test_stream_metrics_throughput() {
        let mut metrics = StreamMetrics::new();

        // 没有数据时没有吞吐量
        assert!(metrics.throughput_bytes_per_sec().is_none());

        metrics.record_chunk(1000);
        sleep(Duration::from_millis(100));
        metrics.finish();

        let throughput = metrics.throughput_bytes_per_sec();
        assert!(throughput.is_some());
        // 1000 bytes in ~100ms = ~10000 bytes/sec
        assert!(throughput.unwrap() > 0.0);
    }

    #[test]
    fn test_stream_metrics_error_rate() {
        let mut metrics = StreamMetrics::new();

        // 没有 chunk 时错误率为 0
        assert_eq!(metrics.error_rate(), 0.0);

        metrics.record_chunk(100);
        metrics.record_chunk(100);
        metrics.record_parse_error();

        // 2 chunks, 1 error = 50% error rate
        assert_eq!(metrics.error_rate(), 0.5);
    }

    #[test]
    fn test_stream_metrics_throttle_rate() {
        let mut metrics = StreamMetrics::new();

        // 没有事件时节流率为 0
        assert_eq!(metrics.throttle_rate(), 0.0);

        metrics.record_chunk(100);
        metrics.record_chunk(100);
        metrics.record_throttled_event();
        metrics.record_throttled_event();

        // 2 chunks, 2 throttled = 50% throttle rate
        assert_eq!(metrics.throttle_rate(), 0.5);
    }

    #[test]
    fn test_stream_metrics_buffer_overflow() {
        let mut metrics = StreamMetrics::new();

        assert_eq!(metrics.buffer_overflow_count, 0);

        metrics.record_buffer_overflow();
        assert_eq!(metrics.buffer_overflow_count, 1);

        metrics.record_buffer_overflow();
        assert_eq!(metrics.buffer_overflow_count, 2);
    }

    #[test]
    fn test_stream_metrics_throttled_events() {
        let mut metrics = StreamMetrics::new();

        assert_eq!(metrics.throttled_event_count, 0);

        metrics.record_throttled_event();
        assert_eq!(metrics.throttled_event_count, 1);

        metrics.record_throttled_events(5);
        assert_eq!(metrics.throttled_event_count, 6);
    }

    #[test]
    fn test_stream_metrics_summary() {
        let mut metrics = StreamMetrics::new();
        metrics.record_chunk(1024);
        metrics.record_chunk(2048);
        metrics.record_throttled_event();
        metrics.finish();

        let summary = metrics.summary();
        assert!(summary.contains("chunks: 2"));
        assert!(summary.contains("bytes: 3072"));
        assert!(summary.contains("throttled: 1"));
    }

    #[test]
    fn test_stream_metrics_serialization() {
        let mut metrics = StreamMetrics::new();
        metrics.record_chunk(100);
        metrics.record_buffer_overflow();
        metrics.record_throttled_event();
        metrics.finish();

        let json = serde_json::to_string(&metrics).unwrap();
        let deserialized: StreamMetrics = serde_json::from_str(&json).unwrap();

        assert_eq!(metrics.chunk_count, deserialized.chunk_count);
        assert_eq!(metrics.total_bytes, deserialized.total_bytes);
        assert_eq!(
            metrics.buffer_overflow_count,
            deserialized.buffer_overflow_count
        );
        assert_eq!(
            metrics.throttled_event_count,
            deserialized.throttled_event_count
        );
    }

    #[test]
    fn test_stream_metrics_log_metrics() {
        let mut metrics = StreamMetrics::new();
        metrics.record_chunk(1024);
        metrics.record_chunk(2048);
        metrics.record_parse_error();
        metrics.record_throttled_event();
        metrics.finish();

        // 这个测试主要确保 log_metrics 不会 panic
        metrics.log_metrics(Some("test-flow-id"));
        metrics.log_metrics(None);
    }
}
