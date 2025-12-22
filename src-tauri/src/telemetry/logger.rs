//! 请求日志记录器
//!
//! 提供请求日志记录、查询和轮转功能

use crate::telemetry::types::{
    ModelStats, ProviderStats, RequestLog, RequestStatus, StatsSummary, TimeRange,
};
use crate::ProviderType;
use chrono::{DateTime, Duration, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::path::PathBuf;

/// 日志记录器错误
#[derive(Debug)]
pub enum LoggerError {
    /// IO 错误
    Io(std::io::Error),
    /// 序列化错误
    Serialization(serde_json::Error),
    /// 日志未找到
    NotFound(String),
    /// 日志目录创建失败
    DirectoryCreation(String),
}

impl std::fmt::Display for LoggerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoggerError::Io(e) => write!(f, "IO 错误: {}", e),
            LoggerError::Serialization(e) => write!(f, "序列化错误: {}", e),
            LoggerError::NotFound(id) => write!(f, "日志未找到: {}", id),
            LoggerError::DirectoryCreation(msg) => write!(f, "日志目录创建失败: {}", msg),
        }
    }
}

impl std::error::Error for LoggerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LoggerError::Io(e) => Some(e),
            LoggerError::Serialization(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for LoggerError {
    fn from(err: std::io::Error) -> Self {
        LoggerError::Io(err)
    }
}

impl From<serde_json::Error> for LoggerError {
    fn from(err: serde_json::Error) -> Self {
        LoggerError::Serialization(err)
    }
}

/// 日志轮转配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRotationConfig {
    /// 内存中保留的最大日志条数
    pub max_memory_logs: usize,
    /// 日志文件保留天数
    pub retention_days: u32,
    /// 单个日志文件最大大小（字节）
    pub max_file_size: u64,
    /// 是否启用文件日志
    pub enable_file_logging: bool,
}

impl Default for LogRotationConfig {
    fn default() -> Self {
        Self {
            max_memory_logs: 10000,
            retention_days: 7,
            max_file_size: 10 * 1024 * 1024, // 10MB
            enable_file_logging: true,
        }
    }
}

/// 请求日志记录器
///
/// 管理请求日志的记录、存储和查询
pub struct RequestLogger {
    /// 内存中的日志队列
    logs: RwLock<VecDeque<RequestLog>>,
    /// 日志轮转配置
    config: LogRotationConfig,
    /// 日志文件目录
    log_dir: PathBuf,
    /// 当前日志文件路径
    current_log_file: RwLock<Option<PathBuf>>,
}

impl RequestLogger {
    /// 创建新的日志记录器
    pub fn new(config: LogRotationConfig) -> Result<Self, LoggerError> {
        let log_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".proxycast")
            .join("request_logs");

        // 创建日志目录
        fs::create_dir_all(&log_dir).map_err(|e| {
            LoggerError::DirectoryCreation(format!("无法创建日志目录 {:?}: {}", log_dir, e))
        })?;

        let logger = Self {
            logs: RwLock::new(VecDeque::with_capacity(config.max_memory_logs)),
            config,
            log_dir,
            current_log_file: RwLock::new(None),
        };

        // 初始化日志文件
        if logger.config.enable_file_logging {
            logger.rotate_log_file_if_needed()?;
        }

        Ok(logger)
    }

    /// 使用默认配置创建日志记录器
    pub fn with_defaults() -> Result<Self, LoggerError> {
        Self::new(LogRotationConfig::default())
    }

    /// 记录请求日志
    pub fn record(&self, log: RequestLog) -> Result<(), LoggerError> {
        // 写入内存
        {
            let mut logs = self.logs.write();
            logs.push_back(log.clone());

            // 内存日志轮转
            while logs.len() > self.config.max_memory_logs {
                logs.pop_front();
            }
        }

        // 写入文件
        if self.config.enable_file_logging {
            self.write_to_file(&log)?;
        }

        Ok(())
    }

    /// 获取所有内存中的日志
    pub fn get_all(&self) -> Vec<RequestLog> {
        self.logs.read().iter().cloned().collect()
    }

    /// 获取指定时间范围内的日志
    pub fn get_by_time_range(&self, range: TimeRange) -> Vec<RequestLog> {
        self.logs
            .read()
            .iter()
            .filter(|log| range.contains(&log.timestamp))
            .cloned()
            .collect()
    }

    /// 按 Provider 过滤日志
    pub fn get_by_provider(&self, provider: ProviderType) -> Vec<RequestLog> {
        self.logs
            .read()
            .iter()
            .filter(|log| log.provider == provider)
            .cloned()
            .collect()
    }

    /// 按模型过滤日志
    pub fn get_by_model(&self, model: &str) -> Vec<RequestLog> {
        self.logs
            .read()
            .iter()
            .filter(|log| log.model == model)
            .cloned()
            .collect()
    }

    /// 按状态过滤日志
    pub fn get_by_status(&self, status: RequestStatus) -> Vec<RequestLog> {
        self.logs
            .read()
            .iter()
            .filter(|log| log.status == status)
            .cloned()
            .collect()
    }

    /// 获取指定 ID 的日志
    pub fn get_by_id(&self, id: &str) -> Option<RequestLog> {
        self.logs.read().iter().find(|log| log.id == id).cloned()
    }

    /// 获取统计摘要
    pub fn summary(&self, range: Option<TimeRange>) -> StatsSummary {
        let logs = match range {
            Some(r) => self.get_by_time_range(r),
            None => self.get_all(),
        };
        StatsSummary::from_logs(&logs)
    }

    /// 按 Provider 分组统计
    pub fn stats_by_provider(
        &self,
        range: Option<TimeRange>,
    ) -> HashMap<ProviderType, ProviderStats> {
        let logs = match range {
            Some(r) => self.get_by_time_range(r),
            None => self.get_all(),
        };

        let mut grouped: HashMap<ProviderType, Vec<RequestLog>> = HashMap::new();
        for log in logs {
            grouped.entry(log.provider).or_default().push(log);
        }

        grouped
            .into_iter()
            .map(|(provider, logs)| (provider, ProviderStats::from_logs(provider, &logs)))
            .collect()
    }

    /// 按模型分组统计
    pub fn stats_by_model(&self, range: Option<TimeRange>) -> HashMap<String, ModelStats> {
        let logs = match range {
            Some(r) => self.get_by_time_range(r),
            None => self.get_all(),
        };

        let mut grouped: HashMap<String, Vec<RequestLog>> = HashMap::new();
        for log in logs {
            grouped.entry(log.model.clone()).or_default().push(log);
        }

        grouped
            .into_iter()
            .map(|(model, logs)| {
                let stats = ModelStats::from_logs(model.clone(), &logs);
                (model, stats)
            })
            .collect()
    }

    /// 获取日志数量
    pub fn len(&self) -> usize {
        self.logs.read().len()
    }

    /// 检查日志是否为空
    pub fn is_empty(&self) -> bool {
        self.logs.read().is_empty()
    }

    /// 清空内存中的日志
    pub fn clear(&self) {
        self.logs.write().clear();
    }

    /// 执行日志轮转（清理过期日志文件）
    pub fn rotate(&self) -> Result<u32, LoggerError> {
        if !self.config.enable_file_logging {
            return Ok(0);
        }

        let cutoff = Utc::now() - Duration::days(self.config.retention_days as i64);
        let mut removed_count = 0;

        // 遍历日志目录
        for entry in fs::read_dir(&self.log_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && path.extension().is_some_and(|ext| ext == "jsonl") {
                // 从文件名解析日期
                if let Some(file_date) = self.parse_log_file_date(&path) {
                    if file_date < cutoff {
                        fs::remove_file(&path)?;
                        removed_count += 1;
                    }
                }
            }
        }

        Ok(removed_count)
    }

    /// 获取日志文件目录
    pub fn log_dir(&self) -> &PathBuf {
        &self.log_dir
    }

    /// 获取当前日志文件路径
    pub fn current_log_file(&self) -> Option<PathBuf> {
        self.current_log_file.read().clone()
    }

    // ========== 私有方法 ==========

    /// 写入日志到文件
    fn write_to_file(&self, log: &RequestLog) -> Result<(), LoggerError> {
        self.rotate_log_file_if_needed()?;

        let file_path = self.current_log_file.read().clone();
        if let Some(path) = file_path {
            let mut file = OpenOptions::new().create(true).append(true).open(&path)?;

            let json = serde_json::to_string(log)?;
            writeln!(file, "{}", json)?;
        }

        Ok(())
    }

    /// 如果需要则轮转日志文件
    fn rotate_log_file_if_needed(&self) -> Result<(), LoggerError> {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let expected_file = self.log_dir.join(format!("requests_{}.jsonl", today));

        let needs_rotation = {
            let current = self.current_log_file.read();
            match &*current {
                None => true,
                Some(path) => {
                    // 检查日期是否变化
                    if *path != expected_file {
                        true
                    } else {
                        // 检查文件大小
                        path.metadata()
                            .map(|m| m.len() >= self.config.max_file_size)
                            .unwrap_or(false)
                    }
                }
            }
        };

        if needs_rotation {
            let mut current = self.current_log_file.write();

            // 如果文件大小超限，创建带序号的新文件
            let new_file = if expected_file.exists()
                && expected_file
                    .metadata()
                    .map(|m| m.len() >= self.config.max_file_size)
                    .unwrap_or(false)
            {
                self.find_next_log_file(&today)?
            } else {
                expected_file
            };

            *current = Some(new_file);
        }

        Ok(())
    }

    /// 查找下一个可用的日志文件名
    fn find_next_log_file(&self, date: &str) -> Result<PathBuf, LoggerError> {
        let mut index = 1;
        loop {
            let file = self
                .log_dir
                .join(format!("requests_{}_{}.jsonl", date, index));
            if !file.exists()
                || file
                    .metadata()
                    .map(|m| m.len() < self.config.max_file_size)
                    .unwrap_or(true)
            {
                return Ok(file);
            }
            index += 1;
            if index > 1000 {
                // 防止无限循环
                return Err(LoggerError::DirectoryCreation(
                    "无法找到可用的日志文件名".to_string(),
                ));
            }
        }
    }

    /// 从日志文件名解析日期
    fn parse_log_file_date(&self, path: &Path) -> Option<DateTime<Utc>> {
        let file_name = path.file_stem()?.to_str()?;
        // 文件名格式: requests_YYYY-MM-DD 或 requests_YYYY-MM-DD_N
        let date_part = file_name.strip_prefix("requests_")?;
        let date_str = if date_part.len() >= 10 {
            &date_part[..10]
        } else {
            return None;
        };

        chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
            .ok()
            .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc())
    }

    /// 从文件加载日志（用于恢复）
    pub fn load_from_file(&self, path: &PathBuf) -> Result<Vec<RequestLog>, LoggerError> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut logs = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if !line.trim().is_empty() {
                if let Ok(log) = serde_json::from_str::<RequestLog>(&line) {
                    logs.push(log);
                }
            }
        }

        Ok(logs)
    }

    /// 加载最近的日志文件到内存
    pub fn load_recent_logs(&self) -> Result<usize, LoggerError> {
        if !self.config.enable_file_logging {
            return Ok(0);
        }

        let cutoff = Utc::now() - Duration::days(1); // 只加载最近一天的
        let mut loaded_count = 0;

        // 收集符合条件的日志文件
        let mut log_files: Vec<PathBuf> = Vec::new();
        for entry in fs::read_dir(&self.log_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && path.extension().is_some_and(|ext| ext == "jsonl") {
                if let Some(file_date) = self.parse_log_file_date(&path) {
                    if file_date >= cutoff {
                        log_files.push(path);
                    }
                }
            }
        }

        // 按文件名排序
        log_files.sort();

        // 加载日志
        for path in log_files {
            let logs = self.load_from_file(&path)?;
            let mut memory_logs = self.logs.write();
            for log in logs {
                memory_logs.push_back(log);
                loaded_count += 1;

                // 保持内存限制
                while memory_logs.len() > self.config.max_memory_logs {
                    memory_logs.pop_front();
                }
            }
        }

        Ok(loaded_count)
    }
}

impl Default for RequestLogger {
    fn default() -> Self {
        Self::with_defaults().expect("Failed to create default RequestLogger")
    }
}
