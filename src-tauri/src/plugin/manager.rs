//! 插件管理器
//!
//! 负责插件的生命周期管理、钩子执行和配置管理

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::RwLock;
use tokio::time::timeout;

use super::loader::PluginLoader;
use super::types::{
    HookResult, PluginConfig, PluginContext, PluginError, PluginInfo, PluginInstance, PluginStatus,
};

/// 插件管理器配置
#[derive(Debug, Clone)]
pub struct PluginManagerConfig {
    /// 默认超时时间 (毫秒)
    pub default_timeout_ms: u64,
    /// 是否启用插件系统
    pub enabled: bool,
    /// 最大并发插件数
    pub max_plugins: usize,
}

impl Default for PluginManagerConfig {
    fn default() -> Self {
        Self {
            default_timeout_ms: 5000,
            enabled: true,
            max_plugins: 50,
        }
    }
}

/// 插件管理器
pub struct PluginManager {
    /// 插件加载器
    loader: PluginLoader,
    /// 已加载的插件
    plugins: DashMap<String, Arc<RwLock<PluginInstance>>>,
    /// 插件配置
    configs: DashMap<String, PluginConfig>,
    /// 管理器配置
    config: PluginManagerConfig,
}

impl PluginManager {
    /// 创建新的插件管理器
    pub fn new(plugins_dir: PathBuf, config: PluginManagerConfig) -> Self {
        Self {
            loader: PluginLoader::new(plugins_dir),
            plugins: DashMap::new(),
            configs: DashMap::new(),
            config,
        }
    }

    /// 使用默认配置创建
    pub fn with_defaults() -> Self {
        Self::new(
            PluginLoader::default_plugins_dir(),
            PluginManagerConfig::default(),
        )
    }

    /// 加载所有插件
    pub async fn load_all(&self) -> Result<Vec<String>, PluginError> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        let configs: HashMap<String, PluginConfig> = self
            .configs
            .iter()
            .map(|r| (r.key().clone(), r.value().clone()))
            .collect();

        let loaded = self.loader.load_all(&configs).await?;
        let mut names = Vec::new();

        for (path, plugin) in loaded {
            let name = plugin.name().to_string();
            let config = configs.get(&name).cloned().unwrap_or_default();

            let mut instance = PluginInstance::new(plugin.clone(), path, config.clone());

            // 初始化插件
            if let Err(e) = Arc::get_mut(&mut instance.plugin)
                .ok_or_else(|| PluginError::InitError("无法获取插件可变引用".to_string()))?
                .init(&config)
                .await
            {
                tracing::warn!("插件 {} 初始化失败: {}", name, e);
                instance.state.status = PluginStatus::Error;
                instance.state.last_error = Some(e.to_string());
            } else {
                instance.state.status = if config.enabled {
                    PluginStatus::Enabled
                } else {
                    PluginStatus::Disabled
                };
            }

            self.plugins
                .insert(name.clone(), Arc::new(RwLock::new(instance)));
            names.push(name);
        }

        Ok(names)
    }

    /// 加载单个插件
    pub async fn load(&self, plugin_dir: &Path) -> Result<String, PluginError> {
        if self.plugins.len() >= self.config.max_plugins {
            return Err(PluginError::LoadError(format!(
                "已达到最大插件数限制: {}",
                self.config.max_plugins
            )));
        }

        let config = PluginConfig::default();
        let plugin = self.loader.load(plugin_dir, &config).await?;
        let name = plugin.name().to_string();

        // 检查是否已加载
        if self.plugins.contains_key(&name) {
            return Err(PluginError::LoadError(format!("插件 {} 已加载", name)));
        }

        let mut instance =
            PluginInstance::new(plugin.clone(), plugin_dir.to_path_buf(), config.clone());

        // 初始化插件
        if let Err(e) = Arc::get_mut(&mut instance.plugin)
            .ok_or_else(|| PluginError::InitError("无法获取插件可变引用".to_string()))?
            .init(&config)
            .await
        {
            instance.state.status = PluginStatus::Error;
            instance.state.last_error = Some(e.to_string());
        } else {
            instance.state.status = PluginStatus::Enabled;
        }

        self.plugins
            .insert(name.clone(), Arc::new(RwLock::new(instance)));
        Ok(name)
    }

    /// 卸载插件
    pub async fn unload(&self, name: &str) -> Result<(), PluginError> {
        let instance = self
            .plugins
            .remove(name)
            .map(|(_, v)| v)
            .ok_or_else(|| PluginError::NotFound(name.to_string()))?;

        // 关闭插件
        let mut inst = instance.write().await;
        if let Some(plugin) = Arc::get_mut(&mut inst.plugin) {
            plugin.shutdown().await?;
        }

        Ok(())
    }

    /// 启用插件
    pub async fn enable(&self, name: &str) -> Result<(), PluginError> {
        let instance = self
            .plugins
            .get(name)
            .ok_or_else(|| PluginError::NotFound(name.to_string()))?;

        let mut inst = instance.write().await;
        inst.config.enabled = true;
        inst.state.status = PluginStatus::Enabled;

        // 更新配置
        self.configs.insert(name.to_string(), inst.config.clone());

        Ok(())
    }

    /// 禁用插件
    pub async fn disable(&self, name: &str) -> Result<(), PluginError> {
        let instance = self
            .plugins
            .get(name)
            .ok_or_else(|| PluginError::NotFound(name.to_string()))?;

        let mut inst = instance.write().await;
        inst.config.enabled = false;
        inst.state.status = PluginStatus::Disabled;

        // 更新配置
        self.configs.insert(name.to_string(), inst.config.clone());

        Ok(())
    }

    /// 更新插件配置
    pub async fn update_config(&self, name: &str, config: PluginConfig) -> Result<(), PluginError> {
        let instance = self
            .plugins
            .get(name)
            .ok_or_else(|| PluginError::NotFound(name.to_string()))?;

        let mut inst = instance.write().await;
        inst.config = config.clone();

        // 更新状态
        if config.enabled && inst.state.status != PluginStatus::Error {
            inst.state.status = PluginStatus::Enabled;
        } else if !config.enabled {
            inst.state.status = PluginStatus::Disabled;
        }

        // 更新配置存储
        self.configs.insert(name.to_string(), config);

        Ok(())
    }

    /// 获取插件配置
    pub fn get_config(&self, name: &str) -> Option<PluginConfig> {
        self.configs.get(name).map(|r| r.value().clone())
    }

    /// 获取插件信息
    pub async fn get_info(&self, name: &str) -> Option<PluginInfo> {
        let instance = self.plugins.get(name)?;
        let inst = instance.read().await;
        Some(inst.info())
    }

    /// 获取所有插件信息
    pub async fn list(&self) -> Vec<PluginInfo> {
        let mut infos = Vec::new();
        for entry in self.plugins.iter() {
            let inst = entry.value().read().await;
            infos.push(inst.info());
        }
        infos
    }

    /// 执行请求前钩子 (带隔离)
    pub async fn run_on_request(
        &self,
        ctx: &mut PluginContext,
        request: &mut serde_json::Value,
    ) -> Vec<HookResult> {
        if !self.config.enabled {
            return Vec::new();
        }

        let mut results = Vec::new();

        for entry in self.plugins.iter() {
            let instance = entry.value().read().await;
            if !instance.is_enabled() {
                continue;
            }

            let timeout_ms = instance.config.timeout_ms;
            let plugin = instance.plugin.clone();
            let plugin_name = plugin.name().to_string();

            // 带超时执行
            let result = match timeout(
                Duration::from_millis(timeout_ms),
                plugin.on_request(ctx, request),
            )
            .await
            {
                Ok(Ok(result)) => result,
                Ok(Err(e)) => {
                    tracing::warn!("插件 {} on_request 执行失败: {}", plugin_name, e);
                    HookResult::failure(e.to_string(), timeout_ms)
                }
                Err(_) => {
                    tracing::warn!("插件 {} on_request 执行超时", plugin_name);
                    HookResult::failure(format!("执行超时 ({}ms)", timeout_ms), timeout_ms)
                }
            };

            // 更新状态
            drop(instance);
            if let Some(inst) = self.plugins.get(&plugin_name) {
                let mut inst = inst.write().await;
                inst.state
                    .record_execution(result.success, result.error.clone());
            }

            results.push(result);
        }

        results
    }

    /// 执行响应后钩子 (带隔离)
    pub async fn run_on_response(
        &self,
        ctx: &mut PluginContext,
        response: &mut serde_json::Value,
    ) -> Vec<HookResult> {
        if !self.config.enabled {
            return Vec::new();
        }

        let mut results = Vec::new();

        for entry in self.plugins.iter() {
            let instance = entry.value().read().await;
            if !instance.is_enabled() {
                continue;
            }

            let timeout_ms = instance.config.timeout_ms;
            let plugin = instance.plugin.clone();
            let plugin_name = plugin.name().to_string();

            // 带超时执行
            let result = match timeout(
                Duration::from_millis(timeout_ms),
                plugin.on_response(ctx, response),
            )
            .await
            {
                Ok(Ok(result)) => result,
                Ok(Err(e)) => {
                    tracing::warn!("插件 {} on_response 执行失败: {}", plugin_name, e);
                    HookResult::failure(e.to_string(), timeout_ms)
                }
                Err(_) => {
                    tracing::warn!("插件 {} on_response 执行超时", plugin_name);
                    HookResult::failure(format!("执行超时 ({}ms)", timeout_ms), timeout_ms)
                }
            };

            // 更新状态
            drop(instance);
            if let Some(inst) = self.plugins.get(&plugin_name) {
                let mut inst = inst.write().await;
                inst.state
                    .record_execution(result.success, result.error.clone());
            }

            results.push(result);
        }

        results
    }

    /// 执行错误钩子 (带隔离)
    pub async fn run_on_error(&self, ctx: &mut PluginContext, error: &str) -> Vec<HookResult> {
        if !self.config.enabled {
            return Vec::new();
        }

        let mut results = Vec::new();

        for entry in self.plugins.iter() {
            let instance = entry.value().read().await;
            if !instance.is_enabled() {
                continue;
            }

            let timeout_ms = instance.config.timeout_ms;
            let plugin = instance.plugin.clone();
            let plugin_name = plugin.name().to_string();

            // 带超时执行
            let result = match timeout(
                Duration::from_millis(timeout_ms),
                plugin.on_error(ctx, error),
            )
            .await
            {
                Ok(Ok(result)) => result,
                Ok(Err(e)) => {
                    tracing::warn!("插件 {} on_error 执行失败: {}", plugin_name, e);
                    HookResult::failure(e.to_string(), timeout_ms)
                }
                Err(_) => {
                    tracing::warn!("插件 {} on_error 执行超时", plugin_name);
                    HookResult::failure(format!("执行超时 ({}ms)", timeout_ms), timeout_ms)
                }
            };

            // 更新状态
            drop(instance);
            if let Some(inst) = self.plugins.get(&plugin_name) {
                let mut inst = inst.write().await;
                inst.state
                    .record_execution(result.success, result.error.clone());
            }

            results.push(result);
        }

        results
    }

    /// 获取已加载插件数量
    pub fn count(&self) -> usize {
        self.plugins.len()
    }

    /// 检查插件是否已加载
    pub fn is_loaded(&self, name: &str) -> bool {
        self.plugins.contains_key(name)
    }

    /// 获取插件目录
    pub fn plugins_dir(&self) -> &Path {
        self.loader.plugins_dir()
    }

    /// 设置插件配置 (批量)
    pub fn set_configs(&self, configs: HashMap<String, PluginConfig>) {
        for (name, config) in configs {
            self.configs.insert(name, config);
        }
    }

    /// 获取所有插件配置
    pub fn get_all_configs(&self) -> HashMap<String, PluginConfig> {
        self.configs
            .iter()
            .map(|r| (r.key().clone(), r.value().clone()))
            .collect()
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::with_defaults()
    }
}
