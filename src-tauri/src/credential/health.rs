//! 凭证健康检查器实现
//!
//! 提供凭证健康状态检查和自动更新功能

use super::pool::{CredentialPool, PoolError};
use super::types::{Credential, CredentialStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 健康状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// 健康
    Healthy,
    /// 不健康
    Unhealthy {
        /// 不健康原因
        reason: String,
        /// 连续失败次数
        consecutive_failures: u32,
    },
    /// 未知（未检查过）
    Unknown,
}

/// 健康检查结果
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    /// 凭证 ID
    pub credential_id: String,
    /// 健康状态
    pub status: HealthStatus,
    /// 检查时间
    pub checked_at: DateTime<Utc>,
    /// 检查延迟（毫秒）
    pub latency_ms: Option<u64>,
}

/// 健康检查配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// 检查间隔
    pub check_interval: Duration,
    /// 连续失败阈值（达到此值标记为不健康）
    pub failure_threshold: u32,
    /// 恢复阈值（连续成功此次数后恢复为健康）
    pub recovery_threshold: u32,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(60),
            failure_threshold: 3,
            recovery_threshold: 1,
        }
    }
}

/// 健康检查器 - 管理凭证健康状态
pub struct HealthChecker {
    /// 配置
    config: HealthCheckConfig,
}

impl HealthChecker {
    /// 创建新的健康检查器
    pub fn new(config: HealthCheckConfig) -> Self {
        Self { config }
    }

    /// 使用默认配置创建健康检查器
    pub fn with_defaults() -> Self {
        Self::new(HealthCheckConfig::default())
    }

    /// 获取配置
    pub fn config(&self) -> &HealthCheckConfig {
        &self.config
    }

    /// 获取失败阈值
    pub fn failure_threshold(&self) -> u32 {
        self.config.failure_threshold
    }

    /// 获取恢复阈值
    pub fn recovery_threshold(&self) -> u32 {
        self.config.recovery_threshold
    }

    /// 检查单个凭证的健康状态
    ///
    /// 根据凭证的统计信息判断健康状态
    pub fn check(&self, credential: &Credential) -> HealthCheckResult {
        let status = self.evaluate_health(credential);

        HealthCheckResult {
            credential_id: credential.id.clone(),
            status,
            checked_at: Utc::now(),
            latency_ms: if credential.stats.successful_requests > 0 {
                Some(credential.stats.avg_latency_ms as u64)
            } else {
                None
            },
        }
    }

    /// 评估凭证健康状态
    fn evaluate_health(&self, credential: &Credential) -> HealthStatus {
        // 如果已经被标记为不健康，返回当前状态
        if let CredentialStatus::Unhealthy { reason } = &credential.status {
            return HealthStatus::Unhealthy {
                reason: reason.clone(),
                consecutive_failures: credential.stats.consecutive_failures,
            };
        }

        // 检查连续失败次数
        if credential.stats.consecutive_failures >= self.config.failure_threshold {
            return HealthStatus::Unhealthy {
                reason: format!(
                    "连续失败 {} 次（阈值: {}）",
                    credential.stats.consecutive_failures, self.config.failure_threshold
                ),
                consecutive_failures: credential.stats.consecutive_failures,
            };
        }

        // 如果没有请求记录，状态未知
        if credential.stats.total_requests == 0 {
            return HealthStatus::Unknown;
        }

        HealthStatus::Healthy
    }

    /// 记录凭证使用失败并更新健康状态
    ///
    /// 如果连续失败次数达到阈值，自动标记为不健康
    ///
    /// # 返回
    /// - `true` 如果凭证被标记为不健康
    /// - `false` 如果凭证仍然健康
    pub fn record_failure(
        &self,
        pool: &CredentialPool,
        credential_id: &str,
    ) -> Result<bool, PoolError> {
        // 记录失败
        pool.record_failure(credential_id)?;

        // 获取更新后的凭证
        let credential = pool
            .get(credential_id)
            .ok_or_else(|| PoolError::CredentialNotFound(credential_id.to_string()))?;

        // 检查是否需要标记为不健康
        if credential.stats.consecutive_failures >= self.config.failure_threshold {
            let reason = format!("连续认证失败 {} 次", credential.stats.consecutive_failures);
            pool.mark_unhealthy(credential_id, reason)?;
            return Ok(true);
        }

        Ok(false)
    }

    /// 记录凭证使用成功并更新健康状态
    ///
    /// 如果凭证之前不健康，成功后会恢复为健康状态
    ///
    /// # 返回
    /// - `true` 如果凭证从不健康恢复为健康
    /// - `false` 如果凭证状态未改变
    pub fn record_success(
        &self,
        pool: &CredentialPool,
        credential_id: &str,
        latency_ms: u64,
    ) -> Result<bool, PoolError> {
        // 获取当前状态
        let was_unhealthy = pool
            .get(credential_id)
            .map(|c| matches!(c.status, CredentialStatus::Unhealthy { .. }))
            .unwrap_or(false);

        // 记录成功
        pool.record_success(credential_id, latency_ms)?;

        // 如果之前不健康，恢复为健康
        if was_unhealthy {
            pool.mark_active(credential_id)?;
            return Ok(true);
        }

        Ok(false)
    }

    /// 批量检查凭证池中所有凭证的健康状态
    pub fn check_all(&self, pool: &CredentialPool) -> Vec<HealthCheckResult> {
        pool.all().iter().map(|cred| self.check(cred)).collect()
    }

    /// 获取池中不健康的凭证数量
    pub fn unhealthy_count(&self, pool: &CredentialPool) -> usize {
        pool.all()
            .iter()
            .filter(|cred| matches!(self.check(cred).status, HealthStatus::Unhealthy { .. }))
            .count()
    }

    /// 检查凭证是否应该被标记为不健康
    ///
    /// 这是一个纯函数，不会修改任何状态
    pub fn should_mark_unhealthy(&self, consecutive_failures: u32) -> bool {
        consecutive_failures >= self.config.failure_threshold
    }

    /// 尝试恢复不健康的凭证
    ///
    /// 将所有不健康的凭证恢复为活跃状态（用于手动恢复）
    pub fn recover_all(&self, pool: &CredentialPool) -> Vec<String> {
        let mut recovered = Vec::new();

        for cred in pool.all() {
            if matches!(cred.status, CredentialStatus::Unhealthy { .. })
                && pool.mark_active(&cred.id).is_ok()
            {
                recovered.push(cred.id.clone());
            }
        }

        recovered
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod health_tests {
    use super::*;
    use crate::credential::CredentialData;
    use crate::ProviderType;

    fn create_test_credential(id: &str) -> Credential {
        Credential::new(
            id.to_string(),
            ProviderType::Kiro,
            CredentialData::ApiKey {
                key: format!("key-{}", id),
                base_url: None,
            },
        )
    }

    #[test]
    fn test_health_checker_new() {
        let checker = HealthChecker::with_defaults();
        assert_eq!(checker.failure_threshold(), 3);
        assert_eq!(checker.recovery_threshold(), 1);
    }

    #[test]
    fn test_health_checker_custom_config() {
        let config = HealthCheckConfig {
            check_interval: Duration::from_secs(30),
            failure_threshold: 5,
            recovery_threshold: 2,
        };
        let checker = HealthChecker::new(config);
        assert_eq!(checker.failure_threshold(), 5);
        assert_eq!(checker.recovery_threshold(), 2);
    }

    #[test]
    fn test_check_healthy_credential() {
        let checker = HealthChecker::with_defaults();
        let mut cred = create_test_credential("test-1");

        // 记录一些成功请求
        cred.stats.record_success(100);
        cred.stats.record_success(150);

        let result = checker.check(&cred);
        assert_eq!(result.credential_id, "test-1");
        assert!(matches!(result.status, HealthStatus::Healthy));
    }

    #[test]
    fn test_check_unknown_credential() {
        let checker = HealthChecker::with_defaults();
        let cred = create_test_credential("test-1");

        // 没有任何请求记录
        let result = checker.check(&cred);
        assert!(matches!(result.status, HealthStatus::Unknown));
    }

    #[test]
    fn test_check_unhealthy_credential() {
        let checker = HealthChecker::with_defaults();
        let mut cred = create_test_credential("test-1");

        // 记录 3 次连续失败
        cred.stats.record_failure();
        cred.stats.record_failure();
        cred.stats.record_failure();

        let result = checker.check(&cred);
        assert!(matches!(result.status, HealthStatus::Unhealthy { .. }));

        if let HealthStatus::Unhealthy {
            consecutive_failures,
            ..
        } = result.status
        {
            assert_eq!(consecutive_failures, 3);
        }
    }

    #[test]
    fn test_record_failure_marks_unhealthy() {
        let checker = HealthChecker::with_defaults();
        let pool = CredentialPool::new(ProviderType::Kiro);
        pool.add(create_test_credential("test-1")).unwrap();

        // 前两次失败不应标记为不健康
        assert!(!checker.record_failure(&pool, "test-1").unwrap());
        assert!(!checker.record_failure(&pool, "test-1").unwrap());

        // 第三次失败应标记为不健康
        assert!(checker.record_failure(&pool, "test-1").unwrap());

        // 验证状态
        let cred = pool.get("test-1").unwrap();
        assert!(matches!(cred.status, CredentialStatus::Unhealthy { .. }));
    }

    #[test]
    fn test_record_success_recovers_unhealthy() {
        let checker = HealthChecker::with_defaults();
        let pool = CredentialPool::new(ProviderType::Kiro);
        pool.add(create_test_credential("test-1")).unwrap();

        // 标记为不健康
        pool.mark_unhealthy("test-1", "test reason".to_string())
            .unwrap();

        // 记录成功应恢复
        let recovered = checker.record_success(&pool, "test-1", 100).unwrap();
        assert!(recovered);

        // 验证状态
        let cred = pool.get("test-1").unwrap();
        assert!(matches!(cred.status, CredentialStatus::Active));
    }

    #[test]
    fn test_check_all() {
        let checker = HealthChecker::with_defaults();
        let pool = CredentialPool::new(ProviderType::Kiro);

        pool.add(create_test_credential("cred-1")).unwrap();
        pool.add(create_test_credential("cred-2")).unwrap();
        pool.add(create_test_credential("cred-3")).unwrap();

        let results = checker.check_all(&pool);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_unhealthy_count() {
        let checker = HealthChecker::with_defaults();
        let pool = CredentialPool::new(ProviderType::Kiro);

        pool.add(create_test_credential("cred-1")).unwrap();
        pool.add(create_test_credential("cred-2")).unwrap();
        pool.add(create_test_credential("cred-3")).unwrap();

        // 标记一个为不健康
        pool.mark_unhealthy("cred-2", "test".to_string()).unwrap();

        assert_eq!(checker.unhealthy_count(&pool), 1);
    }

    #[test]
    fn test_should_mark_unhealthy() {
        let checker = HealthChecker::with_defaults();

        assert!(!checker.should_mark_unhealthy(0));
        assert!(!checker.should_mark_unhealthy(1));
        assert!(!checker.should_mark_unhealthy(2));
        assert!(checker.should_mark_unhealthy(3));
        assert!(checker.should_mark_unhealthy(4));
    }

    #[test]
    fn test_recover_all() {
        let checker = HealthChecker::with_defaults();
        let pool = CredentialPool::new(ProviderType::Kiro);

        pool.add(create_test_credential("cred-1")).unwrap();
        pool.add(create_test_credential("cred-2")).unwrap();
        pool.add(create_test_credential("cred-3")).unwrap();

        // 标记两个为不健康
        pool.mark_unhealthy("cred-1", "test".to_string()).unwrap();
        pool.mark_unhealthy("cred-3", "test".to_string()).unwrap();

        let recovered = checker.recover_all(&pool);
        assert_eq!(recovered.len(), 2);
        assert!(recovered.contains(&"cred-1".to_string()));
        assert!(recovered.contains(&"cred-3".to_string()));

        // 验证所有凭证都是活跃状态
        for cred in pool.all() {
            assert!(matches!(cred.status, CredentialStatus::Active));
        }
    }
}
