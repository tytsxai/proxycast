//! WebSocket 连接生命周期管理
//!
//! 提供心跳检测、优雅关闭和资源清理功能

use super::WsMessage;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// 心跳管理器
#[derive(Debug)]
pub struct HeartbeatManager {
    /// 心跳间隔
    interval: Duration,
    /// 心跳超时
    timeout: Duration,
    /// 最后一次心跳时间（Unix 时间戳毫秒）
    last_heartbeat: AtomicU64,
    /// 是否已停止
    stopped: AtomicBool,
}

impl HeartbeatManager {
    /// 创建新的心跳管理器
    pub fn new(interval_secs: u64, timeout_secs: u64) -> Self {
        Self {
            interval: Duration::from_secs(interval_secs),
            timeout: Duration::from_secs(timeout_secs),
            last_heartbeat: AtomicU64::new(Self::current_timestamp()),
            stopped: AtomicBool::new(false),
        }
    }

    /// 使用默认配置创建
    pub fn with_defaults() -> Self {
        Self::new(30, 60)
    }

    /// 获取当前时间戳（毫秒）
    fn current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    /// 记录心跳
    pub fn record_heartbeat(&self) {
        self.last_heartbeat
            .store(Self::current_timestamp(), Ordering::Relaxed);
    }

    /// 检查是否超时
    pub fn is_timed_out(&self) -> bool {
        let last = self.last_heartbeat.load(Ordering::Relaxed);
        let now = Self::current_timestamp();
        let elapsed = Duration::from_millis(now.saturating_sub(last));
        elapsed > self.timeout
    }

    /// 获取心跳间隔
    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// 获取超时时间
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// 停止心跳管理器
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
    }

    /// 检查是否已停止
    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Relaxed)
    }

    /// 获取自上次心跳以来的时间
    pub fn elapsed_since_last_heartbeat(&self) -> Duration {
        let last = self.last_heartbeat.load(Ordering::Relaxed);
        let now = Self::current_timestamp();
        Duration::from_millis(now.saturating_sub(last))
    }

    /// 创建 Ping 消息
    pub fn create_ping(&self) -> WsMessage {
        WsMessage::Ping {
            timestamp: Self::current_timestamp() as i64,
        }
    }

    /// 创建 Pong 消息
    pub fn create_pong(&self, timestamp: i64) -> WsMessage {
        WsMessage::Pong { timestamp }
    }
}

/// 连接生命周期状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleState {
    /// 连接中
    Connecting,
    /// 已连接
    Connected,
    /// 正在关闭
    Closing,
    /// 已关闭
    Closed,
}

/// 连接生命周期管理器
#[derive(Debug)]
pub struct ConnectionLifecycle {
    /// 连接 ID
    conn_id: String,
    /// 当前状态
    state: std::sync::atomic::AtomicU8,
    /// 心跳管理器
    heartbeat: HeartbeatManager,
    /// 创建时间
    created_at: Instant,
    /// 关闭原因
    close_reason: parking_lot::RwLock<Option<String>>,
}

impl ConnectionLifecycle {
    /// 创建新的生命周期管理器
    pub fn new(conn_id: String, heartbeat_interval: u64, heartbeat_timeout: u64) -> Self {
        Self {
            conn_id,
            state: std::sync::atomic::AtomicU8::new(LifecycleState::Connecting as u8),
            heartbeat: HeartbeatManager::new(heartbeat_interval, heartbeat_timeout),
            created_at: Instant::now(),
            close_reason: parking_lot::RwLock::new(None),
        }
    }

    /// 使用默认配置创建
    pub fn with_defaults(conn_id: String) -> Self {
        Self::new(conn_id, 30, 60)
    }

    /// 获取连接 ID
    pub fn conn_id(&self) -> &str {
        &self.conn_id
    }

    /// 获取当前状态
    pub fn state(&self) -> LifecycleState {
        match self.state.load(Ordering::Relaxed) {
            0 => LifecycleState::Connecting,
            1 => LifecycleState::Connected,
            2 => LifecycleState::Closing,
            _ => LifecycleState::Closed,
        }
    }

    /// 设置状态
    fn set_state(&self, state: LifecycleState) {
        self.state.store(state as u8, Ordering::Relaxed);
    }

    /// 标记为已连接
    pub fn mark_connected(&self) {
        self.set_state(LifecycleState::Connected);
        self.heartbeat.record_heartbeat();
    }

    /// 开始关闭
    pub fn start_closing(&self, reason: Option<String>) {
        self.set_state(LifecycleState::Closing);
        if let Some(r) = reason {
            *self.close_reason.write() = Some(r);
        }
    }

    /// 标记为已关闭
    pub fn mark_closed(&self) {
        self.set_state(LifecycleState::Closed);
        self.heartbeat.stop();
    }

    /// 检查是否应该关闭（心跳超时）
    pub fn should_close(&self) -> bool {
        self.heartbeat.is_timed_out()
    }

    /// 记录心跳
    pub fn on_heartbeat(&self) {
        self.heartbeat.record_heartbeat();
    }

    /// 获取连接时长
    pub fn uptime(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// 获取关闭原因
    pub fn close_reason(&self) -> Option<String> {
        self.close_reason.read().clone()
    }

    /// 获取心跳管理器
    pub fn heartbeat(&self) -> &HeartbeatManager {
        &self.heartbeat
    }

    /// 检查连接是否活跃
    pub fn is_active(&self) -> bool {
        matches!(self.state(), LifecycleState::Connected)
    }
}

/// 优雅关闭处理器
pub struct GracefulShutdown {
    /// 关闭信号发送端
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    /// 关闭超时
    timeout: Duration,
}

impl GracefulShutdown {
    /// 创建新的优雅关闭处理器
    pub fn new(timeout_secs: u64) -> (Self, tokio::sync::oneshot::Receiver<()>) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        (
            Self {
                shutdown_tx: Some(tx),
                timeout: Duration::from_secs(timeout_secs),
            },
            rx,
        )
    }

    /// 触发关闭
    pub fn trigger(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }

    /// 获取超时时间
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// 检查是否已触发关闭
    pub fn is_triggered(&self) -> bool {
        self.shutdown_tx.is_none()
    }
}

/// 资源清理器
pub struct ResourceCleaner {
    /// 清理任务列表
    cleanup_tasks: Vec<Box<dyn FnOnce() + Send>>,
}

impl ResourceCleaner {
    /// 创建新的资源清理器
    pub fn new() -> Self {
        Self {
            cleanup_tasks: Vec::new(),
        }
    }

    /// 注册清理任务
    pub fn register<F>(&mut self, task: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.cleanup_tasks.push(Box::new(task));
    }

    /// 执行所有清理任务
    pub fn cleanup(self) {
        for task in self.cleanup_tasks {
            task();
        }
    }

    /// 获取待清理任务数量
    pub fn pending_count(&self) -> usize {
        self.cleanup_tasks.len()
    }
}

impl Default for ResourceCleaner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heartbeat_manager_new() {
        let hb = HeartbeatManager::new(30, 60);
        assert_eq!(hb.interval(), Duration::from_secs(30));
        assert_eq!(hb.timeout(), Duration::from_secs(60));
        assert!(!hb.is_stopped());
        assert!(!hb.is_timed_out());
    }

    #[test]
    fn test_heartbeat_manager_record() {
        let hb = HeartbeatManager::new(30, 60);
        let before = hb.elapsed_since_last_heartbeat();
        std::thread::sleep(Duration::from_millis(10));
        hb.record_heartbeat();
        let after = hb.elapsed_since_last_heartbeat();
        assert!(after < before || after < Duration::from_millis(5));
    }

    #[test]
    fn test_heartbeat_manager_stop() {
        let hb = HeartbeatManager::new(30, 60);
        assert!(!hb.is_stopped());
        hb.stop();
        assert!(hb.is_stopped());
    }

    #[test]
    fn test_heartbeat_manager_create_ping_pong() {
        let hb = HeartbeatManager::new(30, 60);
        let ping = hb.create_ping();
        match ping {
            WsMessage::Ping { timestamp } => {
                assert!(timestamp > 0);
                let pong = hb.create_pong(timestamp);
                match pong {
                    WsMessage::Pong { timestamp: t } => assert_eq!(t, timestamp),
                    _ => panic!("Expected Pong"),
                }
            }
            _ => panic!("Expected Ping"),
        }
    }

    #[test]
    fn test_connection_lifecycle_states() {
        let lifecycle = ConnectionLifecycle::with_defaults("conn-1".to_string());
        assert_eq!(lifecycle.state(), LifecycleState::Connecting);

        lifecycle.mark_connected();
        assert_eq!(lifecycle.state(), LifecycleState::Connected);
        assert!(lifecycle.is_active());

        lifecycle.start_closing(Some("test reason".to_string()));
        assert_eq!(lifecycle.state(), LifecycleState::Closing);
        assert!(!lifecycle.is_active());
        assert_eq!(lifecycle.close_reason(), Some("test reason".to_string()));

        lifecycle.mark_closed();
        assert_eq!(lifecycle.state(), LifecycleState::Closed);
    }

    #[test]
    fn test_connection_lifecycle_uptime() {
        let lifecycle = ConnectionLifecycle::with_defaults("conn-1".to_string());
        std::thread::sleep(Duration::from_millis(10));
        assert!(lifecycle.uptime() >= Duration::from_millis(10));
    }

    #[test]
    fn test_graceful_shutdown() {
        let (mut shutdown, mut rx) = GracefulShutdown::new(5);
        assert!(!shutdown.is_triggered());
        assert_eq!(shutdown.timeout(), Duration::from_secs(5));

        shutdown.trigger();
        assert!(shutdown.is_triggered());
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn test_resource_cleaner() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let counter = Arc::new(AtomicUsize::new(0));
        let mut cleaner = ResourceCleaner::new();

        let c1 = counter.clone();
        cleaner.register(move || {
            c1.fetch_add(1, Ordering::Relaxed);
        });

        let c2 = counter.clone();
        cleaner.register(move || {
            c2.fetch_add(1, Ordering::Relaxed);
        });

        assert_eq!(cleaner.pending_count(), 2);
        cleaner.cleanup();
        assert_eq!(counter.load(Ordering::Relaxed), 2);
    }
}
