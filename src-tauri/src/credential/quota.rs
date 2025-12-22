//! 配额管理器实现
//!
//! 提供配额超限检测、自动切换和冷却恢复功能

use crate::config::QuotaExceededConfig;
use crate::resilience::{QUOTA_EXCEEDED_KEYWORDS, QUOTA_EXCEEDED_STATUS_CODES};
use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 配额超限记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaExceededRecord {
    /// 凭证 ID
    pub credential_id: String,
    /// 超限时间
    pub exceeded_at: DateTime<Utc>,
    /// 冷却结束时间
    pub cooldown_until: DateTime<Utc>,
    /// 超限原因
    pub reason: String,
}

/// 配额管理器
///
/// 管理凭证的配额超限状态，支持：
/// - 标记凭证为配额超限
/// - 检查凭证是否可用
/// - 自动清理过期的冷却状态
/// - 预览模型回退
#[derive(Debug)]
pub struct QuotaManager {
    /// 配额超限配置
    config: QuotaExceededConfig,
    /// 超限凭证记录（credential_id -> record）
    exceeded_credentials: DashMap<String, QuotaExceededRecord>,
}

impl QuotaManager {
    /// 创建新的配额管理器
    pub fn new(config: QuotaExceededConfig) -> Self {
        Self {
            config,
            exceeded_credentials: DashMap::new(),
        }
    }

    /// 使用默认配置创建配额管理器
    pub fn with_defaults() -> Self {
        Self::new(QuotaExceededConfig::default())
    }

    /// 获取配置
    pub fn config(&self) -> &QuotaExceededConfig {
        &self.config
    }

    /// 更新配置
    pub fn set_config(&mut self, config: QuotaExceededConfig) {
        self.config = config;
    }

    /// 获取冷却时长
    pub fn cooldown_duration(&self) -> Duration {
        Duration::seconds(self.config.cooldown_seconds as i64)
    }

    /// 标记凭证为配额超限
    ///
    /// # 参数
    /// - `credential_id`: 凭证 ID
    /// - `reason`: 超限原因
    ///
    /// # 返回
    /// 配额超限记录
    pub fn mark_quota_exceeded(&self, credential_id: &str, reason: &str) -> QuotaExceededRecord {
        let now = Utc::now();
        let cooldown_until = now + self.cooldown_duration();

        let record = QuotaExceededRecord {
            credential_id: credential_id.to_string(),
            exceeded_at: now,
            cooldown_until,
            reason: reason.to_string(),
        };

        self.exceeded_credentials
            .insert(credential_id.to_string(), record.clone());

        tracing::info!(
            credential_id = %credential_id,
            cooldown_until = %cooldown_until,
            reason = %reason,
            "凭证配额超限，已标记冷却"
        );

        record
    }

    /// 检查凭证是否可用（未超限或已过冷却期）
    ///
    /// # 参数
    /// - `credential_id`: 凭证 ID
    ///
    /// # 返回
    /// - `true`: 凭证可用
    /// - `false`: 凭证处于冷却期
    pub fn is_available(&self, credential_id: &str) -> bool {
        match self.exceeded_credentials.get(credential_id) {
            Some(record) => {
                let now = Utc::now();
                if now >= record.cooldown_until {
                    // 冷却期已过，移除记录
                    drop(record); // 释放读锁
                    self.exceeded_credentials.remove(credential_id);
                    true
                } else {
                    false
                }
            }
            None => true,
        }
    }

    /// 获取凭证的冷却结束时间
    ///
    /// # 参数
    /// - `credential_id`: 凭证 ID
    ///
    /// # 返回
    /// - `Some(DateTime)`: 冷却结束时间
    /// - `None`: 凭证未处于冷却期
    pub fn get_cooldown_until(&self, credential_id: &str) -> Option<DateTime<Utc>> {
        self.exceeded_credentials
            .get(credential_id)
            .map(|r| r.cooldown_until)
    }

    /// 获取凭证的超限记录
    pub fn get_record(&self, credential_id: &str) -> Option<QuotaExceededRecord> {
        self.exceeded_credentials
            .get(credential_id)
            .map(|r| r.clone())
    }

    /// 清理过期的冷却记录
    ///
    /// # 返回
    /// 清理的记录数量
    pub fn cleanup_expired(&self) -> usize {
        let now = Utc::now();
        let mut cleaned = 0;

        // 收集需要移除的 ID
        let expired_ids: Vec<String> = self
            .exceeded_credentials
            .iter()
            .filter(|r| now >= r.cooldown_until)
            .map(|r| r.credential_id.clone())
            .collect();

        // 移除过期记录
        for id in expired_ids {
            self.exceeded_credentials.remove(&id);
            cleaned += 1;
            tracing::debug!(credential_id = %id, "凭证冷却期已过，已恢复可用");
        }

        if cleaned > 0 {
            tracing::info!(count = cleaned, "已清理过期的配额超限记录");
        }

        cleaned
    }

    /// 手动恢复凭证（移除冷却状态）
    ///
    /// # 参数
    /// - `credential_id`: 凭证 ID
    ///
    /// # 返回
    /// - `true`: 成功移除冷却状态
    /// - `false`: 凭证未处于冷却期
    pub fn restore_credential(&self, credential_id: &str) -> bool {
        self.exceeded_credentials.remove(credential_id).is_some()
    }

    /// 获取所有处于冷却期的凭证 ID
    pub fn get_exceeded_credentials(&self) -> Vec<String> {
        self.exceeded_credentials
            .iter()
            .map(|r| r.credential_id.clone())
            .collect()
    }

    /// 获取超限凭证数量
    pub fn exceeded_count(&self) -> usize {
        self.exceeded_credentials.len()
    }

    /// 手动设置凭证的冷却结束时间（仅用于测试）
    #[cfg(test)]
    pub fn set_cooldown_until(&self, credential_id: &str, until: DateTime<Utc>) {
        if let Some(mut record) = self.exceeded_credentials.get_mut(credential_id) {
            record.cooldown_until = until;
        }
    }

    /// 检查是否为配额超限错误
    ///
    /// # 参数
    /// - `status_code`: HTTP 状态码
    /// - `error_message`: 错误消息
    ///
    /// # 返回
    /// - `true`: 是配额超限错误
    /// - `false`: 不是配额超限错误
    pub fn is_quota_exceeded_error(status_code: Option<u16>, error_message: &str) -> bool {
        // 检查状态码
        if let Some(code) = status_code {
            if QUOTA_EXCEEDED_STATUS_CODES.contains(&code) {
                return true;
            }
        }

        // 检查错误消息中的关键词
        let error_lower = error_message.to_lowercase();
        for keyword in QUOTA_EXCEEDED_KEYWORDS {
            if error_lower.contains(keyword) {
                return true;
            }
        }

        false
    }

    /// 获取预览模型名称
    ///
    /// 将模型名称映射到预览版本，例如：
    /// - `gemini-2.5-pro` → `gemini-2.5-pro-preview`
    /// - `claude-3-opus` → `claude-3-opus-preview`
    /// - `gpt-4` → `gpt-4-preview`
    ///
    /// 特殊映射：
    /// - `gemini-2.5-pro` → `gemini-2.5-pro-preview-05-06` (如果存在特定日期版本)
    ///
    /// # 参数
    /// - `model`: 原始模型名称
    ///
    /// # 返回
    /// - `Some(String)`: 预览模型名称
    /// - `None`: 无法生成预览模型名称（已经是预览版本或功能禁用）
    pub fn get_preview_model(&self, model: &str) -> Option<String> {
        if !self.config.switch_preview_model {
            return None;
        }

        // 如果已经是预览版本，返回 None
        if Self::is_preview_model(model) {
            return None;
        }

        // 添加 -preview 后缀
        Some(format!("{}-preview", model))
    }

    /// 检查模型是否为预览版本
    ///
    /// # 参数
    /// - `model`: 模型名称
    ///
    /// # 返回
    /// - `true`: 是预览版本
    /// - `false`: 不是预览版本
    pub fn is_preview_model(model: &str) -> bool {
        model.ends_with("-preview") || model.contains("-preview-")
    }

    /// 获取原始模型名称（从预览版本）
    ///
    /// 将预览模型名称映射回原始版本，例如：
    /// - `gemini-2.5-pro-preview` → `gemini-2.5-pro`
    /// - `gemini-2.5-pro-preview-05-06` → `gemini-2.5-pro`
    ///
    /// # 参数
    /// - `model`: 预览模型名称
    ///
    /// # 返回
    /// - `Some(String)`: 原始模型名称
    /// - `None`: 不是预览版本
    pub fn get_original_model(model: &str) -> Option<String> {
        if !Self::is_preview_model(model) {
            return None;
        }

        // 移除 -preview 后缀或 -preview-xxx 部分
        model.find("-preview").map(|pos| model[..pos].to_string())
    }

    /// 检查是否启用自动切换项目
    pub fn is_switch_project_enabled(&self) -> bool {
        self.config.switch_project
    }

    /// 检查是否启用预览模型回退
    pub fn is_switch_preview_model_enabled(&self) -> bool {
        self.config.switch_preview_model
    }

    /// 获取最早的恢复时间
    ///
    /// # 返回
    /// - `Some(DateTime)`: 最早的冷却结束时间
    /// - `None`: 没有凭证处于冷却期
    pub fn earliest_recovery(&self) -> Option<DateTime<Utc>> {
        self.exceeded_credentials
            .iter()
            .map(|r| r.cooldown_until)
            .min()
    }

    /// 获取剩余冷却时间（秒）
    ///
    /// # 参数
    /// - `credential_id`: 凭证 ID
    ///
    /// # 返回
    /// - `Some(i64)`: 剩余冷却秒数（如果为负数则表示已过期）
    /// - `None`: 凭证未处于冷却期
    pub fn remaining_cooldown_seconds(&self, credential_id: &str) -> Option<i64> {
        self.exceeded_credentials.get(credential_id).map(|r| {
            let now = Utc::now();
            (r.cooldown_until - now).num_seconds()
        })
    }
}

impl Default for QuotaManager {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// 创建共享的配额管理器
pub fn create_shared_quota_manager(config: QuotaExceededConfig) -> Arc<QuotaManager> {
    Arc::new(QuotaManager::new(config))
}

/// 启动配额管理器的定期清理任务
///
/// 在后台定期清理过期的配额超限记录
///
/// # 参数
/// - `manager`: 共享的配额管理器
/// - `interval_secs`: 清理间隔（秒）
///
/// # 返回
/// 取消句柄（drop 时停止清理任务）
pub fn start_quota_cleanup_task(
    manager: Arc<QuotaManager>,
    interval_secs: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;
            let cleaned = manager.cleanup_expired();
            if cleaned > 0 {
                tracing::debug!(cleaned_count = cleaned, "定期清理配额超限记录完成");
            }
        }
    })
}

/// 配额自动切换结果
#[derive(Debug, Clone)]
pub struct QuotaAutoSwitchResult {
    /// 是否成功切换
    pub switched: bool,
    /// 新的凭证 ID（如果切换成功）
    pub new_credential_id: Option<String>,
    /// 是否使用了预览模型
    pub used_preview_model: bool,
    /// 预览模型名称（如果使用了预览模型）
    pub preview_model: Option<String>,
    /// 消息
    pub message: String,
}

impl QuotaAutoSwitchResult {
    /// 创建成功切换的结果
    pub fn switched(new_credential_id: String) -> Self {
        let message = format!("已切换到凭证: {}", new_credential_id);
        Self {
            switched: true,
            new_credential_id: Some(new_credential_id),
            used_preview_model: false,
            preview_model: None,
            message,
        }
    }

    /// 创建使用预览模型的结果
    pub fn preview_model(model: String) -> Self {
        let message = format!("已切换到预览模型: {}", model);
        Self {
            switched: false,
            new_credential_id: None,
            used_preview_model: true,
            preview_model: Some(model),
            message,
        }
    }

    /// 创建未切换的结果
    pub fn not_switched(message: &str) -> Self {
        Self {
            switched: false,
            new_credential_id: None,
            used_preview_model: false,
            preview_model: None,
            message: message.to_string(),
        }
    }

    /// 创建所有凭证耗尽的结果
    pub fn all_exhausted(earliest_recovery: Option<DateTime<Utc>>) -> Self {
        let message = match earliest_recovery {
            Some(time) => format!("所有凭证配额超限，最早恢复时间: {}", time),
            None => "所有凭证配额超限，无可用凭证".to_string(),
        };
        Self {
            switched: false,
            new_credential_id: None,
            used_preview_model: false,
            preview_model: None,
            message,
        }
    }
}

/// 所有凭证耗尽错误
#[derive(Debug, Clone)]
pub struct AllCredentialsExhaustedError {
    /// 最早恢复时间
    pub earliest_recovery: Option<DateTime<Utc>>,
    /// 重试等待秒数（用于 Retry-After 头）
    pub retry_after_seconds: Option<u64>,
    /// 错误消息
    pub message: String,
}

impl AllCredentialsExhaustedError {
    /// 创建新的错误
    pub fn new(earliest_recovery: Option<DateTime<Utc>>) -> Self {
        let retry_after_seconds = earliest_recovery.map(|time| {
            let now = Utc::now();
            if time > now {
                (time - now).num_seconds().max(0) as u64
            } else {
                0
            }
        });

        let message = match earliest_recovery {
            Some(time) => format!(
                "所有凭证配额超限，最早恢复时间: {}",
                time.format("%Y-%m-%d %H:%M:%S UTC")
            ),
            None => "所有凭证配额超限，无可用凭证".to_string(),
        };

        Self {
            earliest_recovery,
            retry_after_seconds,
            message,
        }
    }

    /// 获取 HTTP 状态码
    pub fn status_code(&self) -> u16 {
        503 // Service Unavailable
    }

    /// 获取 Retry-After 头的值
    pub fn retry_after_header(&self) -> Option<String> {
        self.retry_after_seconds.map(|s| s.to_string())
    }
}

impl std::fmt::Display for AllCredentialsExhaustedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AllCredentialsExhaustedError {}

/// 实现 IntoResponse 以便在 axum 处理器中直接返回 503 响应
///
/// 响应格式：
/// - HTTP 状态码: 503 Service Unavailable
/// - Retry-After 头: 如果有最早恢复时间，则包含等待秒数
/// - 响应体: JSON 格式的错误信息
impl axum::response::IntoResponse for AllCredentialsExhaustedError {
    fn into_response(self) -> axum::response::Response {
        use axum::http::{header, StatusCode};
        use axum::Json;

        let json_body = serde_json::json!({
            "error": {
                "message": self.message,
                "type": "all_credentials_exhausted",
                "code": 503,
                "retry_after_seconds": self.retry_after_seconds
            }
        });

        let mut response = (StatusCode::SERVICE_UNAVAILABLE, Json(json_body)).into_response();

        // 添加 Retry-After 头
        if let Some(retry_after) = self.retry_after_header() {
            if let Ok(header_value) = retry_after.parse() {
                response
                    .headers_mut()
                    .insert(header::RETRY_AFTER, header_value);
            }
        }

        response
    }
}

impl QuotaManager {
    /// 处理配额超限并尝试自动切换
    ///
    /// 当凭证配额超限时，根据配置执行以下策略：
    /// 1. 如果 switch_project 启用，尝试切换到下一个可用凭证
    /// 2. 如果 switch_preview_model 启用，尝试使用预览模型
    ///
    /// # 参数
    /// - `failed_credential_id`: 失败的凭证 ID
    /// - `model`: 请求的模型名称
    /// - `available_credential_ids`: 所有可用的凭证 ID 列表
    /// - `error_message`: 错误消息
    ///
    /// # 返回
    /// 自动切换结果
    pub fn handle_quota_exceeded(
        &self,
        failed_credential_id: &str,
        model: &str,
        available_credential_ids: &[String],
        error_message: &str,
    ) -> QuotaAutoSwitchResult {
        // 标记当前凭证为配额超限
        self.mark_quota_exceeded(failed_credential_id, error_message);

        // 如果启用了自动切换项目
        if self.config.switch_project {
            // 查找下一个可用的凭证（排除已超限的）
            for cred_id in available_credential_ids {
                if cred_id != failed_credential_id && self.is_available(cred_id) {
                    tracing::info!(
                        from_credential = %failed_credential_id,
                        to_credential = %cred_id,
                        "配额超限，自动切换凭证"
                    );
                    return QuotaAutoSwitchResult::switched(cred_id.clone());
                }
            }
        }

        // 如果没有可用凭证，尝试使用预览模型
        if self.config.switch_preview_model {
            if let Some(preview) = self.get_preview_model(model) {
                tracing::info!(
                    original_model = %model,
                    preview_model = %preview,
                    "配额超限，切换到预览模型"
                );
                return QuotaAutoSwitchResult::preview_model(preview);
            }
        }

        // 所有凭证都不可用
        let earliest = self.earliest_recovery();
        tracing::warn!(
            credential_id = %failed_credential_id,
            earliest_recovery = ?earliest,
            "所有凭证配额超限"
        );
        QuotaAutoSwitchResult::all_exhausted(earliest)
    }

    /// 选择下一个可用凭证
    ///
    /// 从可用凭证列表中选择一个未处于配额超限状态的凭证
    ///
    /// # 参数
    /// - `available_credential_ids`: 所有可用的凭证 ID 列表
    ///
    /// # 返回
    /// - `Some(String)`: 可用的凭证 ID
    /// - `None`: 没有可用凭证
    pub fn select_available_credential(
        &self,
        available_credential_ids: &[String],
    ) -> Option<String> {
        for cred_id in available_credential_ids {
            if self.is_available(cred_id) {
                return Some(cred_id.clone());
            }
        }
        None
    }

    /// 过滤出可用的凭证 ID 列表
    ///
    /// # 参数
    /// - `credential_ids`: 所有凭证 ID 列表
    ///
    /// # 返回
    /// 未处于配额超限状态的凭证 ID 列表
    pub fn filter_available_credentials(&self, credential_ids: &[String]) -> Vec<String> {
        credential_ids
            .iter()
            .filter(|id| self.is_available(id))
            .cloned()
            .collect()
    }

    /// 检查是否所有凭证都已耗尽
    ///
    /// # 参数
    /// - `credential_ids`: 所有凭证 ID 列表
    ///
    /// # 返回
    /// - `Ok(())`: 有可用凭证
    /// - `Err(AllCredentialsExhaustedError)`: 所有凭证都已耗尽
    pub fn check_all_exhausted(
        &self,
        credential_ids: &[String],
    ) -> Result<(), AllCredentialsExhaustedError> {
        let available = self.filter_available_credentials(credential_ids);
        if available.is_empty() {
            Err(AllCredentialsExhaustedError::new(self.earliest_recovery()))
        } else {
            Ok(())
        }
    }

    /// 获取所有凭证耗尽时的错误响应
    ///
    /// # 返回
    /// 包含 503 状态码和 Retry-After 头的错误
    pub fn get_exhausted_error(&self) -> AllCredentialsExhaustedError {
        AllCredentialsExhaustedError::new(self.earliest_recovery())
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_quota_auto_switch_result_switched() {
        let result = QuotaAutoSwitchResult::switched("cred-2".to_string());
        assert!(result.switched);
        assert_eq!(result.new_credential_id, Some("cred-2".to_string()));
        assert!(!result.used_preview_model);
        assert!(result.preview_model.is_none());
    }

    #[test]
    fn test_quota_auto_switch_result_preview_model() {
        let result = QuotaAutoSwitchResult::preview_model("gemini-2.5-pro-preview".to_string());
        assert!(!result.switched);
        assert!(result.new_credential_id.is_none());
        assert!(result.used_preview_model);
        assert_eq!(
            result.preview_model,
            Some("gemini-2.5-pro-preview".to_string())
        );
    }

    #[test]
    fn test_quota_auto_switch_result_not_switched() {
        let result = QuotaAutoSwitchResult::not_switched("No available credentials");
        assert!(!result.switched);
        assert!(result.new_credential_id.is_none());
        assert!(!result.used_preview_model);
        assert!(result.preview_model.is_none());
    }

    #[test]
    fn test_quota_auto_switch_result_all_exhausted() {
        let result = QuotaAutoSwitchResult::all_exhausted(None);
        assert!(!result.switched);
        assert!(result.new_credential_id.is_none());
        assert!(!result.used_preview_model);
        assert!(result.message.contains("无可用凭证"));
    }

    #[test]
    fn test_handle_quota_exceeded_switch_project() {
        let config = QuotaExceededConfig {
            switch_project: true,
            switch_preview_model: false,
            cooldown_seconds: 300,
        };
        let manager = QuotaManager::new(config);

        let available = vec![
            "cred-1".to_string(),
            "cred-2".to_string(),
            "cred-3".to_string(),
        ];

        let result = manager.handle_quota_exceeded(
            "cred-1",
            "gemini-2.5-pro",
            &available,
            "Rate limit exceeded",
        );

        assert!(result.switched);
        assert_eq!(result.new_credential_id, Some("cred-2".to_string()));
        assert!(!result.used_preview_model);
    }

    #[test]
    fn test_handle_quota_exceeded_switch_preview_model() {
        let config = QuotaExceededConfig {
            switch_project: false,
            switch_preview_model: true,
            cooldown_seconds: 300,
        };
        let manager = QuotaManager::new(config);

        let available = vec!["cred-1".to_string()];

        let result = manager.handle_quota_exceeded(
            "cred-1",
            "gemini-2.5-pro",
            &available,
            "Rate limit exceeded",
        );

        assert!(!result.switched);
        assert!(result.used_preview_model);
        assert_eq!(
            result.preview_model,
            Some("gemini-2.5-pro-preview".to_string())
        );
    }

    #[test]
    fn test_handle_quota_exceeded_all_exhausted() {
        let config = QuotaExceededConfig {
            switch_project: true,
            switch_preview_model: false,
            cooldown_seconds: 300,
        };
        let manager = QuotaManager::new(config);

        // 标记所有凭证为超限
        manager.mark_quota_exceeded("cred-1", "test");
        manager.mark_quota_exceeded("cred-2", "test");

        let available = vec!["cred-1".to_string(), "cred-2".to_string()];

        let result = manager.handle_quota_exceeded(
            "cred-1",
            "gemini-2.5-pro",
            &available,
            "Rate limit exceeded",
        );

        assert!(!result.switched);
        assert!(!result.used_preview_model);
        assert!(result.message.contains("所有凭证配额超限"));
    }

    #[test]
    fn test_select_available_credential() {
        let manager = QuotaManager::with_defaults();

        // 标记 cred-1 为超限
        manager.mark_quota_exceeded("cred-1", "test");

        let available = vec![
            "cred-1".to_string(),
            "cred-2".to_string(),
            "cred-3".to_string(),
        ];

        let selected = manager.select_available_credential(&available);
        assert_eq!(selected, Some("cred-2".to_string()));
    }

    #[test]
    fn test_filter_available_credentials() {
        let manager = QuotaManager::with_defaults();

        // 标记 cred-1 和 cred-3 为超限
        manager.mark_quota_exceeded("cred-1", "test");
        manager.mark_quota_exceeded("cred-3", "test");

        let all = vec![
            "cred-1".to_string(),
            "cred-2".to_string(),
            "cred-3".to_string(),
            "cred-4".to_string(),
        ];

        let available = manager.filter_available_credentials(&all);
        assert_eq!(available, vec!["cred-2".to_string(), "cred-4".to_string()]);
    }

    #[test]
    fn test_all_credentials_exhausted_error() {
        let error = AllCredentialsExhaustedError::new(None);
        assert_eq!(error.status_code(), 503);
        assert!(error.retry_after_header().is_none());
        assert!(error.message.contains("无可用凭证"));
    }

    #[test]
    fn test_all_credentials_exhausted_error_with_recovery() {
        let recovery_time = Utc::now() + Duration::seconds(300);
        let error = AllCredentialsExhaustedError::new(Some(recovery_time));

        assert_eq!(error.status_code(), 503);
        assert!(error.retry_after_header().is_some());

        let retry_after = error.retry_after_seconds.unwrap();
        assert!(retry_after > 0);
        assert!(retry_after <= 300);
    }

    #[test]
    fn test_check_all_exhausted_has_available() {
        let manager = QuotaManager::with_defaults();

        // 标记部分凭证为超限
        manager.mark_quota_exceeded("cred-1", "test");

        let all = vec!["cred-1".to_string(), "cred-2".to_string()];

        let result = manager.check_all_exhausted(&all);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_all_exhausted_none_available() {
        let manager = QuotaManager::with_defaults();

        // 标记所有凭证为超限
        manager.mark_quota_exceeded("cred-1", "test");
        manager.mark_quota_exceeded("cred-2", "test");

        let all = vec!["cred-1".to_string(), "cred-2".to_string()];

        let result = manager.check_all_exhausted(&all);
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert_eq!(error.status_code(), 503);
        assert!(error.earliest_recovery.is_some());
    }

    #[test]
    fn test_get_exhausted_error() {
        let manager = QuotaManager::with_defaults();

        // 标记凭证为超限
        manager.mark_quota_exceeded("cred-1", "test");

        let error = manager.get_exhausted_error();
        assert_eq!(error.status_code(), 503);
        assert!(error.earliest_recovery.is_some());
    }

    #[test]
    fn test_quota_manager_new() {
        let config = QuotaExceededConfig {
            switch_project: true,
            switch_preview_model: true,
            cooldown_seconds: 300,
        };
        let manager = QuotaManager::new(config.clone());

        assert_eq!(manager.config().cooldown_seconds, 300);
        assert!(manager.config().switch_project);
        assert!(manager.config().switch_preview_model);
        assert_eq!(manager.exceeded_count(), 0);
    }

    #[test]
    fn test_quota_manager_mark_exceeded() {
        let manager = QuotaManager::with_defaults();

        let record = manager.mark_quota_exceeded("cred-1", "Rate limit exceeded");

        assert_eq!(record.credential_id, "cred-1");
        assert_eq!(record.reason, "Rate limit exceeded");
        assert!(record.cooldown_until > Utc::now());
        assert_eq!(manager.exceeded_count(), 1);
    }

    #[test]
    fn test_quota_manager_is_available() {
        let config = QuotaExceededConfig {
            switch_project: true,
            switch_preview_model: true,
            cooldown_seconds: 1, // 1 秒冷却
        };
        let manager = QuotaManager::new(config);

        // 未标记的凭证应该可用
        assert!(manager.is_available("cred-1"));

        // 标记后应该不可用
        manager.mark_quota_exceeded("cred-1", "test");
        assert!(!manager.is_available("cred-1"));

        // 等待冷却期过后应该可用
        std::thread::sleep(std::time::Duration::from_secs(2));
        assert!(manager.is_available("cred-1"));
    }

    #[test]
    fn test_quota_manager_cleanup_expired() {
        let config = QuotaExceededConfig {
            switch_project: true,
            switch_preview_model: true,
            cooldown_seconds: 0, // 立即过期
        };
        let manager = QuotaManager::new(config);

        // 标记多个凭证
        manager.mark_quota_exceeded("cred-1", "test");
        manager.mark_quota_exceeded("cred-2", "test");
        manager.mark_quota_exceeded("cred-3", "test");

        assert_eq!(manager.exceeded_count(), 3);

        // 等待一小段时间确保过期
        std::thread::sleep(std::time::Duration::from_millis(100));

        // 清理过期记录
        let cleaned = manager.cleanup_expired();
        assert_eq!(cleaned, 3);
        assert_eq!(manager.exceeded_count(), 0);
    }

    #[test]
    fn test_quota_manager_restore_credential() {
        let manager = QuotaManager::with_defaults();

        manager.mark_quota_exceeded("cred-1", "test");
        assert!(!manager.is_available("cred-1"));

        // 手动恢复
        let restored = manager.restore_credential("cred-1");
        assert!(restored);
        assert!(manager.is_available("cred-1"));

        // 再次恢复应该返回 false
        let restored = manager.restore_credential("cred-1");
        assert!(!restored);
    }

    #[test]
    fn test_quota_manager_is_quota_exceeded_error() {
        // 429 状态码
        assert!(QuotaManager::is_quota_exceeded_error(Some(429), ""));

        // 关键词检测
        assert!(QuotaManager::is_quota_exceeded_error(
            Some(400),
            "Rate limit exceeded"
        ));
        assert!(QuotaManager::is_quota_exceeded_error(
            Some(400),
            "Quota exceeded for this API"
        ));
        assert!(QuotaManager::is_quota_exceeded_error(
            Some(400),
            "Too many requests"
        ));

        // 非配额超限错误
        assert!(!QuotaManager::is_quota_exceeded_error(
            Some(400),
            "Bad Request"
        ));
        assert!(!QuotaManager::is_quota_exceeded_error(
            Some(500),
            "Internal Server Error"
        ));
    }

    #[test]
    fn test_quota_manager_get_preview_model() {
        let manager = QuotaManager::with_defaults();

        // 正常模型应该返回预览版本
        assert_eq!(
            manager.get_preview_model("gemini-2.5-pro"),
            Some("gemini-2.5-pro-preview".to_string())
        );
        assert_eq!(
            manager.get_preview_model("claude-3-opus"),
            Some("claude-3-opus-preview".to_string())
        );

        // 已经是预览版本应该返回 None
        assert_eq!(manager.get_preview_model("gemini-2.5-pro-preview"), None);
        assert_eq!(
            manager.get_preview_model("claude-3-opus-preview-20240101"),
            None
        );
    }

    #[test]
    fn test_quota_manager_get_preview_model_disabled() {
        let config = QuotaExceededConfig {
            switch_project: true,
            switch_preview_model: false, // 禁用预览模型
            cooldown_seconds: 300,
        };
        let manager = QuotaManager::new(config);

        // 禁用时应该返回 None
        assert_eq!(manager.get_preview_model("gemini-2.5-pro"), None);
    }

    #[test]
    fn test_is_preview_model() {
        // 预览版本
        assert!(QuotaManager::is_preview_model("gemini-2.5-pro-preview"));
        assert!(QuotaManager::is_preview_model(
            "claude-3-opus-preview-20240101"
        ));
        assert!(QuotaManager::is_preview_model("gpt-4-preview"));

        // 非预览版本
        assert!(!QuotaManager::is_preview_model("gemini-2.5-pro"));
        assert!(!QuotaManager::is_preview_model("claude-3-opus"));
        assert!(!QuotaManager::is_preview_model("gpt-4"));
    }

    #[test]
    fn test_get_original_model() {
        // 从预览版本获取原始版本
        assert_eq!(
            QuotaManager::get_original_model("gemini-2.5-pro-preview"),
            Some("gemini-2.5-pro".to_string())
        );
        assert_eq!(
            QuotaManager::get_original_model("claude-3-opus-preview-20240101"),
            Some("claude-3-opus".to_string())
        );
        assert_eq!(
            QuotaManager::get_original_model("gpt-4-preview"),
            Some("gpt-4".to_string())
        );

        // 非预览版本应该返回 None
        assert_eq!(QuotaManager::get_original_model("gemini-2.5-pro"), None);
        assert_eq!(QuotaManager::get_original_model("claude-3-opus"), None);
    }

    #[test]
    fn test_quota_manager_earliest_recovery() {
        let config = QuotaExceededConfig {
            switch_project: true,
            switch_preview_model: true,
            cooldown_seconds: 300,
        };
        let manager = QuotaManager::new(config);

        // 没有超限凭证时应该返回 None
        assert!(manager.earliest_recovery().is_none());

        // 标记凭证后应该返回最早的恢复时间
        manager.mark_quota_exceeded("cred-1", "test");
        let recovery = manager.earliest_recovery();
        assert!(recovery.is_some());
    }

    #[test]
    fn test_quota_manager_remaining_cooldown_seconds() {
        let config = QuotaExceededConfig {
            switch_project: true,
            switch_preview_model: true,
            cooldown_seconds: 300,
        };
        let manager = QuotaManager::new(config);

        // 未标记的凭证应该返回 None
        assert!(manager.remaining_cooldown_seconds("cred-1").is_none());

        // 标记后应该返回剩余秒数
        manager.mark_quota_exceeded("cred-1", "test");
        let remaining = manager.remaining_cooldown_seconds("cred-1");
        assert!(remaining.is_some());
        assert!(remaining.unwrap() > 0);
        assert!(remaining.unwrap() <= 300);
    }

    #[test]
    fn test_all_credentials_exhausted_into_response() {
        use axum::http::{header, StatusCode};
        use axum::response::IntoResponse;

        // 测试无恢复时间的情况
        let error = AllCredentialsExhaustedError::new(None);
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(response.headers().get(header::RETRY_AFTER).is_none());

        // 测试有恢复时间的情况
        let recovery_time = Utc::now() + Duration::seconds(300);
        let error = AllCredentialsExhaustedError::new(Some(recovery_time));
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let retry_after = response.headers().get(header::RETRY_AFTER);
        assert!(retry_after.is_some());

        // 验证 Retry-After 值在合理范围内
        let retry_value: u64 = retry_after.unwrap().to_str().unwrap().parse().unwrap();
        assert!(retry_value > 0);
        assert!(retry_value <= 300);
    }
}
