//! Provider 调用步骤
//!
//! 集成重试、故障转移和超时控制

use super::traits::{PipelineStep, StepError};
use crate::processor::RequestContext;
use crate::resilience::{
    Failover, FailoverConfig, FailoverManager, Retrier, RetryConfig, TimeoutConfig,
    TimeoutController, TimeoutError,
};
use crate::services::provider_pool_service::ProviderPoolService;
use crate::ProviderType;
use async_trait::async_trait;
use std::future::Future;
use std::sync::Arc;

/// Provider 调用结果
#[derive(Debug, Clone)]
pub struct ProviderCallResult {
    /// 响应内容
    pub response: serde_json::Value,
    /// HTTP 状态码
    pub status_code: u16,
    /// 延迟（毫秒）
    pub latency_ms: u64,
    /// 使用的凭证 ID
    pub credential_id: Option<String>,
}

/// Provider 调用错误
#[derive(Debug, Clone)]
pub struct ProviderCallError {
    /// 错误消息
    pub message: String,
    /// HTTP 状态码（如果有）
    pub status_code: Option<u16>,
    /// 是否可重试
    pub retryable: bool,
    /// 是否应触发故障转移
    pub should_failover: bool,
}

impl ProviderCallError {
    /// 创建可重试错误
    pub fn retryable(message: impl Into<String>, status_code: Option<u16>) -> Self {
        Self {
            message: message.into(),
            status_code,
            retryable: true,
            should_failover: false,
        }
    }

    /// 创建需要故障转移的错误
    pub fn failover(message: impl Into<String>, status_code: Option<u16>) -> Self {
        Self {
            message: message.into(),
            status_code,
            retryable: false,
            should_failover: true,
        }
    }

    /// 创建不可恢复错误
    pub fn fatal(message: impl Into<String>, status_code: Option<u16>) -> Self {
        Self {
            message: message.into(),
            status_code,
            retryable: false,
            should_failover: false,
        }
    }

    /// 检查是否为配额超限错误
    pub fn is_quota_exceeded(&self) -> bool {
        Failover::is_quota_exceeded(self.status_code, &self.message)
    }
}

/// Provider 调用步骤
///
/// 包含重试、故障转移和超时控制的 Provider 调用
pub struct ProviderStep {
    /// 重试器
    retrier: Arc<Retrier>,
    /// 故障转移器
    failover: Arc<Failover>,
    /// 超时控制器
    timeout: Arc<TimeoutController>,
    /// 凭证池服务
    pool_service: Arc<ProviderPoolService>,
}

impl ProviderStep {
    /// 创建新的 Provider 步骤
    pub fn new(
        retrier: Arc<Retrier>,
        failover: Arc<Failover>,
        timeout: Arc<TimeoutController>,
        pool_service: Arc<ProviderPoolService>,
    ) -> Self {
        Self {
            retrier,
            failover,
            timeout,
            pool_service,
        }
    }

    /// 使用默认配置创建
    pub fn with_defaults(pool_service: Arc<ProviderPoolService>) -> Self {
        Self {
            retrier: Arc::new(Retrier::with_defaults()),
            failover: Arc::new(Failover::new(FailoverConfig::default())),
            timeout: Arc::new(TimeoutController::with_defaults()),
            pool_service,
        }
    }

    /// 使用自定义配置创建
    pub fn with_config(
        retry_config: RetryConfig,
        failover_config: FailoverConfig,
        timeout_config: TimeoutConfig,
        pool_service: Arc<ProviderPoolService>,
    ) -> Self {
        Self {
            retrier: Arc::new(Retrier::new(retry_config)),
            failover: Arc::new(Failover::new(failover_config)),
            timeout: Arc::new(TimeoutController::new(timeout_config)),
            pool_service,
        }
    }

    /// 获取重试器
    pub fn retrier(&self) -> &Retrier {
        &self.retrier
    }

    /// 获取故障转移器
    pub fn failover(&self) -> &Failover {
        &self.failover
    }

    /// 获取超时控制器
    pub fn timeout(&self) -> &TimeoutController {
        &self.timeout
    }

    /// 获取凭证池服务
    pub fn pool_service(&self) -> &ProviderPoolService {
        &self.pool_service
    }

    /// 带重试执行 Provider 调用
    ///
    /// 使用 Retrier 包装 Provider 调用，自动处理可重试错误
    ///
    /// # Arguments
    /// * `ctx` - 请求上下文
    /// * `operation` - Provider 调用操作
    ///
    /// # Returns
    /// 成功返回调用结果，失败返回错误
    pub async fn execute_with_retry<F, Fut>(
        &self,
        ctx: &mut RequestContext,
        mut operation: F,
    ) -> Result<ProviderCallResult, ProviderCallError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<ProviderCallResult, ProviderCallError>>,
    {
        let max_retries = self.retrier.config().max_retries;
        let mut attempts = 0u32;

        loop {
            attempts += 1;

            match operation().await {
                Ok(result) => return Ok(result),
                Err(err) => {
                    // 增加重试计数
                    ctx.increment_retry();

                    tracing::warn!(
                        "[RETRY] request_id={} attempt={}/{} error={} status={:?} retryable={}",
                        ctx.request_id,
                        attempts,
                        max_retries + 1,
                        err.message,
                        err.status_code,
                        err.retryable
                    );

                    // 如果不可重试，立即返回
                    if !err.retryable {
                        return Err(err);
                    }

                    // 检查状态码是否可重试
                    let should_retry = err
                        .status_code
                        .is_none_or(|code| self.retrier.config().is_retryable(code));

                    let should_failover = err.should_failover || err.is_quota_exceeded();

                    if !should_retry || attempts > max_retries {
                        return Err(ProviderCallError {
                            message: err.message,
                            status_code: err.status_code,
                            retryable: false,
                            should_failover,
                        });
                    }

                    // 等待退避时间
                    let delay = self.retrier.backoff_delay(attempts - 1);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    /// 带超时执行 Provider 调用
    ///
    /// 使用 TimeoutController 包装 Provider 调用，自动处理超时
    ///
    /// # Arguments
    /// * `ctx` - 请求上下文
    /// * `operation` - Provider 调用操作
    ///
    /// # Returns
    /// 成功返回调用结果，失败返回错误
    pub async fn execute_with_timeout<F>(
        &self,
        ctx: &RequestContext,
        operation: F,
    ) -> Result<ProviderCallResult, ProviderCallError>
    where
        F: Future<Output = Result<ProviderCallResult, ProviderCallError>>,
    {
        let timeout_result = self.timeout.execute_with_timeout(operation).await;

        match timeout_result {
            Ok(call_result) => call_result,
            Err(timeout_err) => {
                let timeout_ms = match &timeout_err {
                    TimeoutError::RequestTimeout { timeout_ms, .. } => *timeout_ms,
                    TimeoutError::StreamIdleTimeout { timeout_ms, .. } => *timeout_ms,
                    TimeoutError::Cancelled => 0,
                };

                tracing::warn!(
                    "[TIMEOUT] request_id={} error={} timeout_ms={}",
                    ctx.request_id,
                    timeout_err,
                    timeout_ms
                );

                Err(ProviderCallError {
                    message: timeout_err.to_string(),
                    status_code: Some(408),
                    retryable: true,
                    should_failover: false,
                })
            }
        }
    }

    /// 带故障转移执行 Provider 调用
    ///
    /// 使用 Failover 处理 Provider 失败，自动切换到其他 Provider
    ///
    /// # Arguments
    /// * `ctx` - 请求上下文
    /// * `error` - Provider 调用错误
    /// * `available_providers` - 可用的 Provider 列表
    ///
    /// # Returns
    /// 如果可以故障转移，返回新的 Provider；否则返回 None
    pub fn handle_failover(
        &self,
        ctx: &RequestContext,
        error: &ProviderCallError,
        available_providers: &[ProviderType],
    ) -> Option<ProviderType> {
        let current_provider = ctx.provider?;

        let result = self.failover.handle_failure(
            current_provider,
            error.status_code,
            &error.message,
            available_providers,
        );

        if result.switched {
            tracing::info!(
                "[FAILOVER] request_id={} from={} to={:?} reason={:?}",
                ctx.request_id,
                current_provider,
                result.new_provider,
                result.failure_type
            );
            result.new_provider
        } else {
            tracing::warn!(
                "[FAILOVER] request_id={} provider={} no_switch reason={}",
                ctx.request_id,
                current_provider,
                result.message
            );
            None
        }
    }

    /// 带重试、超时和故障转移执行完整的 Provider 调用
    ///
    /// 这是主要的调用入口，集成了所有容错机制
    ///
    /// # Arguments
    /// * `ctx` - 请求上下文
    /// * `operation` - Provider 调用操作工厂
    /// * `available_providers` - 可用的 Provider 列表
    ///
    /// # Returns
    /// 成功返回调用结果，失败返回 StepError
    pub async fn execute_with_resilience<F, Fut>(
        &self,
        ctx: &mut RequestContext,
        mut operation_factory: F,
        available_providers: &[ProviderType],
    ) -> Result<ProviderCallResult, StepError>
    where
        F: FnMut(ProviderType) -> Fut,
        Fut: Future<Output = Result<ProviderCallResult, ProviderCallError>>,
    {
        let mut failover_manager = FailoverManager::new(self.failover.config().clone());
        let mut current_provider = ctx.provider.unwrap_or(ProviderType::Kiro);
        let max_failover_attempts = available_providers.len();
        let mut failover_attempts = 0;
        let max_retries = self.retrier.config().max_retries;

        'failover: loop {
            // 更新上下文中的 Provider
            ctx.set_provider(current_provider);
            ctx.retry_count = 0; // 重置重试计数

            tracing::info!(
                "[PROVIDER] request_id={} provider={} model={} failover_attempt={}",
                ctx.request_id,
                current_provider,
                ctx.resolved_model,
                failover_attempts
            );

            // 重试循环
            let mut retry_attempts = 0u32;
            let result: Result<ProviderCallResult, ProviderCallError> = loop {
                retry_attempts += 1;

                // 带超时执行调用
                let call_result = self
                    .execute_with_timeout(ctx, operation_factory(current_provider))
                    .await;

                match call_result {
                    Ok(result) => break Ok(result),
                    Err(err) => {
                        ctx.increment_retry();

                        tracing::warn!(
                            "[RETRY] request_id={} attempt={}/{} error={} status={:?} retryable={}",
                            ctx.request_id,
                            retry_attempts,
                            max_retries + 1,
                            err.message,
                            err.status_code,
                            err.retryable
                        );

                        // 如果不可重试，立即返回错误
                        if !err.retryable {
                            break Err(err);
                        }

                        // 检查状态码是否可重试
                        let should_retry = err
                            .status_code
                            .is_none_or(|code| self.retrier.config().is_retryable(code));

                        let should_failover = err.should_failover || err.is_quota_exceeded();

                        if !should_retry || retry_attempts > max_retries {
                            break Err(ProviderCallError {
                                message: err.message,
                                status_code: err.status_code,
                                retryable: false,
                                should_failover,
                            });
                        }

                        // 等待退避时间
                        let delay = self.retrier.backoff_delay(retry_attempts - 1);
                        tokio::time::sleep(delay).await;
                    }
                }
            };

            match result {
                Ok(call_result) => {
                    return Ok(call_result);
                }
                Err(err) => {
                    // 检查是否应该故障转移
                    if err.should_failover || err.is_quota_exceeded() {
                        failover_attempts += 1;

                        if failover_attempts >= max_failover_attempts {
                            tracing::error!(
                                "[PROVIDER] request_id={} all_providers_failed attempts={}",
                                ctx.request_id,
                                failover_attempts
                            );
                            return Err(StepError::Provider(format!(
                                "所有 Provider 都失败: {}",
                                err.message
                            )));
                        }

                        // 尝试故障转移
                        let failover_result = failover_manager.handle_failure_and_switch(
                            current_provider,
                            err.status_code,
                            &err.message,
                            available_providers,
                        );

                        if let Some(new_provider) = failover_result.new_provider {
                            tracing::info!(
                                "[FAILOVER] request_id={} from={} to={} reason={:?}",
                                ctx.request_id,
                                current_provider,
                                new_provider,
                                failover_result.failure_type
                            );
                            current_provider = new_provider;
                            continue 'failover;
                        }
                    }

                    // 无法恢复，返回错误
                    return Err(StepError::Provider(err.message));
                }
            }
        }
    }

    /// 检查错误是否为配额超限
    pub fn is_quota_exceeded_error(&self, error: &ProviderCallError) -> bool {
        error.is_quota_exceeded()
    }

    /// 检查状态码是否可重试
    pub fn is_retryable_status(&self, status_code: u16) -> bool {
        self.retrier.config().is_retryable(status_code)
    }
}

#[async_trait]
impl PipelineStep for ProviderStep {
    async fn execute(
        &self,
        ctx: &mut RequestContext,
        _payload: &mut serde_json::Value,
    ) -> Result<(), StepError> {
        // 注意：实际的 Provider 调用逻辑在 server.rs 中实现
        // 这里的 execute 方法主要用于管道步骤的统一接口
        // 实际调用应使用 execute_with_resilience 方法

        tracing::info!(
            "[PROVIDER] request_id={} provider={:?} model={} retry_count={}",
            ctx.request_id,
            ctx.provider,
            ctx.resolved_model,
            ctx.retry_count
        );

        // 占位实现 - 实际调用通过 execute_with_resilience 进行
        Ok(())
    }

    fn name(&self) -> &str {
        "provider"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_provider_step_new() {
        let pool_service = Arc::new(ProviderPoolService::new());
        let step = ProviderStep::with_defaults(pool_service);

        assert_eq!(step.name(), "provider");
        assert!(step.is_enabled());
    }

    #[tokio::test]
    async fn test_provider_step_execute() {
        let pool_service = Arc::new(ProviderPoolService::new());
        let step = ProviderStep::with_defaults(pool_service);

        let mut ctx = RequestContext::new("claude-sonnet-4-5".to_string());
        let mut payload = serde_json::json!({"model": "claude-sonnet-4-5"});

        let result = step.execute(&mut ctx, &mut payload).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_provider_step_with_config() {
        let pool_service = Arc::new(ProviderPoolService::new());
        let retry_config = RetryConfig::new(5, 500, 10000);
        let failover_config = FailoverConfig::new(true, true);
        let timeout_config = TimeoutConfig::new(60000, 15000);

        let step = ProviderStep::with_config(
            retry_config.clone(),
            failover_config.clone(),
            timeout_config.clone(),
            pool_service,
        );

        assert_eq!(step.retrier().config().max_retries, 5);
        assert_eq!(step.retrier().config().base_delay_ms, 500);
        assert!(step.failover().config().auto_switch);
        assert_eq!(step.timeout().config().request_timeout_ms, 60000);
    }

    #[test]
    fn test_provider_call_error_retryable() {
        let err = ProviderCallError::retryable("Connection timeout", Some(408));
        assert!(err.retryable);
        assert!(!err.should_failover);
        assert_eq!(err.status_code, Some(408));
    }

    #[test]
    fn test_provider_call_error_failover() {
        let err = ProviderCallError::failover("Rate limit exceeded", Some(429));
        assert!(!err.retryable);
        assert!(err.should_failover);
        assert!(err.is_quota_exceeded());
    }

    #[test]
    fn test_provider_call_error_fatal() {
        let err = ProviderCallError::fatal("Invalid API key", Some(401));
        assert!(!err.retryable);
        assert!(!err.should_failover);
    }

    #[test]
    fn test_is_quota_exceeded_by_status() {
        let err = ProviderCallError::retryable("Error", Some(429));
        assert!(err.is_quota_exceeded());
    }

    #[test]
    fn test_is_quota_exceeded_by_message() {
        let err = ProviderCallError::retryable("Rate limit exceeded", Some(400));
        assert!(err.is_quota_exceeded());

        let err2 = ProviderCallError::retryable("Quota exceeded for this API", Some(400));
        assert!(err2.is_quota_exceeded());
    }

    #[test]
    fn test_is_retryable_status() {
        let pool_service = Arc::new(ProviderPoolService::new());
        let step = ProviderStep::with_defaults(pool_service);

        // 可重试状态码
        assert!(step.is_retryable_status(408));
        assert!(step.is_retryable_status(429));
        assert!(step.is_retryable_status(500));
        assert!(step.is_retryable_status(502));
        assert!(step.is_retryable_status(503));
        assert!(step.is_retryable_status(504));

        // 不可重试状态码
        assert!(!step.is_retryable_status(200));
        assert!(!step.is_retryable_status(400));
        assert!(!step.is_retryable_status(401));
        assert!(!step.is_retryable_status(403));
        assert!(!step.is_retryable_status(404));
    }

    #[tokio::test]
    async fn test_execute_with_retry_success() {
        let pool_service = Arc::new(ProviderPoolService::new());
        let step = ProviderStep::with_defaults(pool_service);
        let mut ctx = RequestContext::new("test-model".to_string());

        let result = step
            .execute_with_retry(&mut ctx, || async {
                Ok(ProviderCallResult {
                    response: serde_json::json!({"content": "Hello"}),
                    status_code: 200,
                    latency_ms: 100,
                    credential_id: Some("cred-1".to_string()),
                })
            })
            .await;

        assert!(result.is_ok());
        let call_result = result.unwrap();
        assert_eq!(call_result.status_code, 200);
    }

    #[tokio::test]
    async fn test_execute_with_retry_non_retryable_error() {
        let pool_service = Arc::new(ProviderPoolService::new());
        let step = ProviderStep::with_defaults(pool_service);
        let mut ctx = RequestContext::new("test-model".to_string());

        let result = step
            .execute_with_retry(&mut ctx, || async {
                Err(ProviderCallError::fatal("Invalid API key", Some(401)))
            })
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status_code, Some(401)); // 保留原始状态码
        assert!(!err.retryable);
    }

    #[tokio::test]
    async fn test_handle_failover() {
        let pool_service = Arc::new(ProviderPoolService::new());
        let step = ProviderStep::with_defaults(pool_service);
        let mut ctx = RequestContext::new("test-model".to_string());
        ctx.set_provider(ProviderType::Kiro);

        let error = ProviderCallError::failover("Rate limit exceeded", Some(429));
        let available = vec![ProviderType::Kiro, ProviderType::Gemini, ProviderType::Qwen];

        let new_provider = step.handle_failover(&ctx, &error, &available);

        assert!(new_provider.is_some());
        assert_eq!(new_provider.unwrap(), ProviderType::Gemini);
    }

    #[tokio::test]
    async fn test_handle_failover_no_alternative() {
        let pool_service = Arc::new(ProviderPoolService::new());
        let step = ProviderStep::with_defaults(pool_service);
        let mut ctx = RequestContext::new("test-model".to_string());
        ctx.set_provider(ProviderType::Kiro);

        let error = ProviderCallError::failover("Rate limit exceeded", Some(429));
        let available = vec![ProviderType::Kiro]; // 只有一个 Provider

        let new_provider = step.handle_failover(&ctx, &error, &available);

        assert!(new_provider.is_none());
    }

    #[tokio::test]
    async fn test_execute_with_timeout_success() {
        let pool_service = Arc::new(ProviderPoolService::new());
        let timeout_config = TimeoutConfig::new(5000, 1000);
        let step = ProviderStep::with_config(
            RetryConfig::default(),
            FailoverConfig::default(),
            timeout_config,
            pool_service,
        );
        let ctx = RequestContext::new("test-model".to_string());

        let result = step
            .execute_with_timeout(&ctx, async {
                Ok(ProviderCallResult {
                    response: serde_json::json!({"content": "Hello"}),
                    status_code: 200,
                    latency_ms: 50,
                    credential_id: None,
                })
            })
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_with_timeout_timeout() {
        let pool_service = Arc::new(ProviderPoolService::new());
        let timeout_config = TimeoutConfig::new(50, 0); // 50ms 超时
        let step = ProviderStep::with_config(
            RetryConfig::default(),
            FailoverConfig::default(),
            timeout_config,
            pool_service,
        );
        let ctx = RequestContext::new("test-model".to_string());

        let result = step
            .execute_with_timeout(&ctx, async {
                tokio::time::sleep(Duration::from_millis(200)).await;
                Ok(ProviderCallResult {
                    response: serde_json::json!({}),
                    status_code: 200,
                    latency_ms: 200,
                    credential_id: None,
                })
            })
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status_code, Some(408));
        assert!(err.retryable);
    }
}
