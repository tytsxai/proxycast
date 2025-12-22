use crate::config::{
    Config, ConfigManager, ExportBundle, ExportOptions as ExportServiceOptions, ExportService,
    ImportOptions as ImportServiceOptions, ImportService, ValidationResult,
};
use crate::models::AppType;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigStatus {
    pub exists: bool,
    pub path: String,
    pub has_env: bool,
}

/// Get the config directory path for an app type
fn get_config_dir(app_type: &AppType) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    match app_type {
        AppType::Claude => Some(home.join(".claude")),
        AppType::Codex => Some(home.join(".codex")),
        AppType::Gemini => Some(home.join(".gemini")),
        AppType::ProxyCast => dirs::config_dir().map(|d| d.join("proxycast")),
    }
}

#[tauri::command]
pub fn get_config_status(app_type: String) -> Result<ConfigStatus, String> {
    let app = app_type.parse::<AppType>().map_err(|e| e.to_string())?;
    let config_dir = get_config_dir(&app).ok_or("Cannot determine config directory")?;

    let main_config = match app {
        AppType::Claude => config_dir.join("settings.json"),
        AppType::Codex => config_dir.join("auth.json"),
        AppType::Gemini => config_dir.join(".env"),
        AppType::ProxyCast => config_dir.join("config.yaml"),
    };

    let has_env = match app {
        AppType::Claude => {
            config_dir.join("settings.json").exists()
                && std::fs::read_to_string(config_dir.join("settings.json"))
                    .map(|s| s.contains("env"))
                    .unwrap_or(false)
        }
        AppType::Codex => config_dir.join("auth.json").exists(),
        AppType::Gemini => config_dir.join(".env").exists(),
        AppType::ProxyCast => {
            config_dir.join("config.yaml").exists() || config_dir.join("config.json").exists()
        }
    };

    let exists = match app {
        AppType::ProxyCast => {
            config_dir.join("config.yaml").exists() || config_dir.join("config.json").exists()
        }
        _ => main_config.exists(),
    };

    Ok(ConfigStatus {
        exists,
        path: config_dir.to_string_lossy().to_string(),
        has_env,
    })
}

#[tauri::command]
pub fn get_config_dir_path(app_type: String) -> Result<String, String> {
    let app = app_type.parse::<AppType>().map_err(|e| e.to_string())?;
    let config_dir = get_config_dir(&app).ok_or("Cannot determine config directory")?;
    Ok(config_dir.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn open_config_folder(_handle: AppHandle, app_type: String) -> Result<bool, String> {
    let app = app_type.parse::<AppType>().map_err(|e| e.to_string())?;
    let config_dir = get_config_dir(&app).ok_or("Cannot determine config directory")?;

    if !config_dir.exists() {
        std::fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&config_dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&config_dir)
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&config_dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(true)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolVersion {
    pub name: String,
    pub version: Option<String>,
    pub installed: bool,
}

/// 检测工具版本的辅助函数
fn check_tool_version(command: &str, args: &[&str]) -> Option<String> {
    // 在 Windows 上，先尝试直接执行命令
    let mut cmd = std::process::Command::new(command);
    cmd.args(args);

    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let output = cmd.output().ok();

    // 如果直接执行失败，在 Windows 上尝试通过 PowerShell 执行
    #[cfg(target_os = "windows")]
    let output = output.or_else(|| {
        std::process::Command::new("powershell")
            .args(["-Command", &format!("{} {}", command, args.join(" "))])
            .creation_flags(0x08000000)
            .output()
            .ok()
    });

    output
        .and_then(|o| {
            if o.status.success() {
                // 先尝试 stdout，失败则尝试 stderr
                String::from_utf8(o.stdout.clone())
                    .or_else(|_| String::from_utf8(o.stderr))
                    .ok()
            } else {
                None
            }
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[tauri::command]
pub async fn get_tool_versions() -> Result<Vec<ToolVersion>, String> {
    let mut versions = Vec::new();

    // 定义要检测的工具列表
    let tools = vec![
        ("Claude Code", "claude", vec!["--version"]),
        ("Codex", "codex", vec!["--version"]),
        ("Gemini CLI", "gemini", vec!["--version"]),
    ];

    for (name, command, args) in tools {
        let version = check_tool_version(command, &args);

        versions.push(ToolVersion {
            name: name.to_string(),
            version: version.clone(),
            installed: version.is_some(),
        });
    }

    Ok(versions)
}

#[tauri::command]
pub async fn get_auto_launch_status(app: AppHandle) -> Result<bool, String> {
    let autostart_manager = app.autolaunch();
    autostart_manager
        .is_enabled()
        .map_err(|e| format!("Failed to get autostart status: {e}"))
}

#[tauri::command]
pub async fn set_auto_launch(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let autostart_manager = app.autolaunch();

    if enabled {
        autostart_manager
            .enable()
            .map_err(|e| format!("Failed to enable autostart: {e}"))?;
    } else {
        autostart_manager
            .disable()
            .map_err(|e| format!("Failed to disable autostart: {e}"))?;
    }

    Ok(enabled)
}

// ============ Config Import/Export Commands ============

/// 配置导出选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportOptions {
    /// 是否脱敏敏感信息（API 密钥等）
    pub redact_secrets: bool,
}

/// 配置导出结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    /// YAML 配置内容
    pub content: String,
    /// 建议的文件名
    pub suggested_filename: String,
}

/// 导出配置为 YAML 字符串
///
/// # Arguments
/// * `config` - 当前配置
/// * `redact_secrets` - 是否脱敏敏感信息
#[tauri::command]
pub fn export_config(config: Config, redact_secrets: bool) -> Result<ExportResult, String> {
    let manager = ConfigManager::new(PathBuf::from("temp.yaml"));
    let mut manager_with_config = manager;
    manager_with_config.set_config(config);

    let content = manager_with_config
        .export(redact_secrets)
        .map_err(|e| e.to_string())?;

    // 生成带时间戳的文件名
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let suffix = if redact_secrets { "_redacted" } else { "" };
    let suggested_filename = format!("proxycast_config_{}{}.yaml", timestamp, suffix);

    Ok(ExportResult {
        content,
        suggested_filename,
    })
}

/// 配置导入选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportOptions {
    /// 是否合并到现有配置（true）或替换（false）
    pub merge: bool,
}

/// 配置导入结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    /// 是否成功
    pub success: bool,
    /// 导入后的配置
    pub config: Config,
    /// 警告信息（如果有）
    pub warnings: Vec<String>,
}

/// 验证配置 YAML 格式
///
/// # Arguments
/// * `yaml_content` - YAML 配置字符串
#[tauri::command]
pub fn validate_config_yaml(yaml_content: String) -> Result<Config, String> {
    ConfigManager::parse_yaml(&yaml_content).map_err(|e| e.to_string())
}

/// 导入配置
///
/// # Arguments
/// * `current_config` - 当前配置
/// * `yaml_content` - 要导入的 YAML 配置字符串
/// * `merge` - 是否合并到现有配置（true）或替换（false）
#[tauri::command]
pub fn import_config(
    current_config: Config,
    yaml_content: String,
    merge: bool,
) -> Result<ImportResult, String> {
    let mut manager = ConfigManager::new(PathBuf::from("temp.yaml"));
    manager.set_config(current_config);

    let mut warnings = Vec::new();

    // 先验证 YAML 格式
    let imported_config = ConfigManager::parse_yaml(&yaml_content).map_err(|e| e.to_string())?;

    // 检查是否包含脱敏的密钥
    if imported_config.server.api_key == "***REDACTED***" {
        warnings.push("导入的配置包含脱敏的 API 密钥，将保留原有值".to_string());
    }
    if imported_config
        .providers
        .openai
        .api_key
        .as_ref()
        .map(|k| k == "***REDACTED***")
        .unwrap_or(false)
    {
        warnings.push("导入的配置包含脱敏的 OpenAI API 密钥，将保留原有值".to_string());
    }
    if imported_config
        .providers
        .claude
        .api_key
        .as_ref()
        .map(|k| k == "***REDACTED***")
        .unwrap_or(false)
    {
        warnings.push("导入的配置包含脱敏的 Claude API 密钥，将保留原有值".to_string());
    }

    // 执行导入
    manager
        .import(&yaml_content, merge)
        .map_err(|e| e.to_string())?;

    // 如果导入的配置包含脱敏的密钥，恢复原有值
    let final_config = manager.config().clone();

    Ok(ImportResult {
        success: true,
        config: final_config,
        warnings,
    })
}

/// 获取配置文件路径信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigPathInfo {
    /// YAML 配置文件路径
    pub yaml_path: String,
    /// JSON 配置文件路径（旧版）
    pub json_path: String,
    /// YAML 配置是否存在
    pub yaml_exists: bool,
    /// JSON 配置是否存在
    pub json_exists: bool,
}

/// 获取配置文件路径信息
#[tauri::command]
pub fn get_config_paths() -> Result<ConfigPathInfo, String> {
    let yaml_path = ConfigManager::default_config_path();
    let json_path = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("proxycast")
        .join("config.json");

    Ok(ConfigPathInfo {
        yaml_path: yaml_path.to_string_lossy().to_string(),
        json_path: json_path.to_string_lossy().to_string(),
        yaml_exists: yaml_path.exists(),
        json_exists: json_path.exists(),
    })
}

// ============ Enhanced Export/Import Commands (using ExportService/ImportService) ============

/// 统一导出选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedExportOptions {
    /// 是否包含配置
    pub include_config: bool,
    /// 是否包含凭证
    pub include_credentials: bool,
    /// 是否脱敏敏感信息
    pub redact_secrets: bool,
}

/// 统一导出结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedExportResult {
    /// 导出包内容（JSON 格式）
    pub content: String,
    /// 建议的文件名
    pub suggested_filename: String,
    /// 是否已脱敏
    pub redacted: bool,
    /// 是否包含配置
    pub has_config: bool,
    /// 是否包含凭证
    pub has_credentials: bool,
}

/// 导出完整的配置和凭证包
///
/// # Arguments
/// * `config` - 当前配置
/// * `options` - 导出选项
///
/// # Requirements: 3.1, 3.2
#[tauri::command]
pub fn export_bundle(
    config: Config,
    options: UnifiedExportOptions,
) -> Result<UnifiedExportResult, String> {
    let export_options = ExportServiceOptions {
        include_config: options.include_config,
        include_credentials: options.include_credentials,
        redact_secrets: options.redact_secrets,
    };

    // 获取应用版本
    let app_version = env!("CARGO_PKG_VERSION").to_string();

    let bundle =
        ExportService::export(&config, &export_options, &app_version).map_err(|e| e.to_string())?;

    let content = bundle.to_json().map_err(|e| e.to_string())?;

    // 生成带时间戳的文件名
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let suffix = if options.redact_secrets {
        "_redacted"
    } else {
        ""
    };
    let scope = match (options.include_config, options.include_credentials) {
        (true, true) => "full",
        (true, false) => "config",
        (false, true) => "credentials",
        (false, false) => "empty",
    };
    let suggested_filename = format!("proxycast_{}_{}{}.json", scope, timestamp, suffix);

    Ok(UnifiedExportResult {
        content,
        suggested_filename,
        redacted: bundle.redacted,
        has_config: bundle.has_config(),
        has_credentials: bundle.has_credentials(),
    })
}

/// 仅导出配置为 YAML
///
/// # Arguments
/// * `config` - 当前配置
/// * `redact_secrets` - 是否脱敏敏感信息
///
/// # Requirements: 3.1, 5.1
#[tauri::command]
pub fn export_config_yaml(config: Config, redact_secrets: bool) -> Result<ExportResult, String> {
    let content = ExportService::export_yaml(&config, redact_secrets).map_err(|e| e.to_string())?;

    // 生成带时间戳的文件名
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let suffix = if redact_secrets { "_redacted" } else { "" };
    let suggested_filename = format!("proxycast_config_{}{}.yaml", timestamp, suffix);

    Ok(ExportResult {
        content,
        suggested_filename,
    })
}

/// 验证导入内容
///
/// # Arguments
/// * `content` - 导入内容（JSON 导出包或 YAML 配置）
///
/// # Requirements: 4.1, 4.2
#[tauri::command]
pub fn validate_import(content: String) -> Result<ValidationResult, String> {
    Ok(ImportService::validate(&content))
}

/// 导入完整的导出包
///
/// # Arguments
/// * `current_config` - 当前配置
/// * `content` - 导出包内容（JSON 格式）
/// * `merge` - 是否合并到现有配置
///
/// # Requirements: 4.1, 4.3
#[tauri::command]
pub fn import_bundle(
    current_config: Config,
    content: String,
    merge: bool,
) -> Result<ImportResult, String> {
    // 首先尝试解析为 ExportBundle
    if let Ok(bundle) = ExportBundle::from_json(&content) {
        let options = ImportServiceOptions { merge };
        let result =
            ImportService::import(&bundle, &current_config, &options, &current_config.auth_dir)
                .map_err(|e| e.to_string())?;

        return Ok(ImportResult {
            success: result.success,
            config: result.config,
            warnings: result.warnings,
        });
    }

    // 尝试解析为 YAML 配置
    let options = ImportServiceOptions { merge };
    let result = ImportService::import_yaml(&content, &current_config, &options)
        .map_err(|e| e.to_string())?;

    Ok(ImportResult {
        success: result.success,
        config: result.config,
        warnings: result.warnings,
    })
}

// ============ Path Utility Commands ============

/// 展开路径中的 tilde (~) 为用户主目录
///
/// # Arguments
/// * `path` - 要展开的路径字符串
///
/// # Returns
/// 展开后的完整路径字符串
///
/// # Requirements: 2.3
#[tauri::command]
pub fn expand_path(path: String) -> Result<String, String> {
    use crate::config::expand_tilde;

    let expanded = expand_tilde(&path);
    Ok(expanded.to_string_lossy().to_string())
}

/// 打开认证目录
///
/// # Arguments
/// * `path` - 认证目录路径（支持 tilde 展开）
///
/// # Requirements: 2.2
#[tauri::command]
pub async fn open_auth_dir(path: String) -> Result<bool, String> {
    use crate::config::expand_tilde;

    let expanded = expand_tilde(&path);

    // 确保目录存在
    if !expanded.exists() {
        std::fs::create_dir_all(&expanded).map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&expanded)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&expanded)
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&expanded)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(true)
}

// ============ Version Check Commands ============

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionCheckResult {
    pub current: String,
    pub latest: Option<String>,
    #[serde(rename = "hasUpdate")]
    pub has_update: bool,
    #[serde(rename = "downloadUrl")]
    pub download_url: Option<String>,
    pub error: Option<String>,
}

/// 检查应用更新
///
/// 从 GitHub Releases API 获取最新版本信息并与当前版本比较
#[tauri::command]
pub async fn check_for_updates() -> Result<VersionCheckResult, String> {
    const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
    const GITHUB_API_URL: &str =
        "https://api.github.com/repos/aiclientproxy/proxycast/releases/latest";

    let client = reqwest::Client::new();

    match client
        .get(GITHUB_API_URL)
        .header("User-Agent", "ProxyCast")
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<serde_json::Value>().await {
                    Ok(data) => {
                        let latest_version = data["tag_name"]
                            .as_str()
                            .unwrap_or("")
                            .trim_start_matches('v');

                        let download_url = data["html_url"].as_str().map(|s| s.to_string());

                        let has_update = version_compare(CURRENT_VERSION, latest_version);

                        Ok(VersionCheckResult {
                            current: CURRENT_VERSION.to_string(),
                            latest: Some(latest_version.to_string()),
                            has_update,
                            download_url,
                            error: None,
                        })
                    }
                    Err(e) => Ok(VersionCheckResult {
                        current: CURRENT_VERSION.to_string(),
                        latest: None,
                        has_update: false,
                        download_url: None,
                        error: Some(format!("解析响应失败: {}", e)),
                    }),
                }
            } else {
                Ok(VersionCheckResult {
                    current: CURRENT_VERSION.to_string(),
                    latest: None,
                    has_update: false,
                    download_url: None,
                    error: Some(format!("GitHub API 请求失败: {}", response.status())),
                })
            }
        }
        Err(e) => Ok(VersionCheckResult {
            current: CURRENT_VERSION.to_string(),
            latest: None,
            has_update: false,
            download_url: None,
            error: Some(format!("网络请求失败: {}", e)),
        }),
    }
}

/// 简单的版本比较函数
/// 返回 true 如果 latest > current
fn version_compare(current: &str, latest: &str) -> bool {
    // 移除 'v' 前缀
    let current = current.trim_start_matches('v');
    let latest = latest.trim_start_matches('v');

    let current_parts: Vec<u32> = current.split('.').filter_map(|s| s.parse().ok()).collect();
    let latest_parts: Vec<u32> = latest.split('.').filter_map(|s| s.parse().ok()).collect();

    let max_len = current_parts.len().max(latest_parts.len());

    for i in 0..max_len {
        let current_part = current_parts.get(i).unwrap_or(&0);
        let latest_part = latest_parts.get(i).unwrap_or(&0);

        if latest_part > current_part {
            return true;
        } else if latest_part < current_part {
            return false;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_compare() {
        // 测试版本比较逻辑
        assert!(version_compare("0.14.0", "0.14.1"));
        assert!(version_compare("0.14.0", "0.15.0"));
        assert!(version_compare("0.14.0", "1.0.0"));
        assert!(!version_compare("0.14.1", "0.14.0"));
        assert!(!version_compare("0.14.0", "0.14.0"));
        assert!(!version_compare("1.0.0", "0.14.0"));
    }

    #[test]
    fn test_get_platform_patterns() {
        let patterns = get_platform_patterns();

        // 在支持的平台上应该返回非空的模式列表
        #[cfg(any(
            all(
                target_os = "windows",
                any(target_arch = "x86_64", target_arch = "aarch64")
            ),
            all(
                target_os = "macos",
                any(target_arch = "x86_64", target_arch = "aarch64")
            ),
            all(
                target_os = "linux",
                any(target_arch = "x86_64", target_arch = "aarch64")
            )
        ))]
        {
            assert!(!patterns.is_empty());
        }

        // 在不支持的平台上应该返回空列表
        #[cfg(not(any(
            all(
                target_os = "windows",
                any(target_arch = "x86_64", target_arch = "aarch64")
            ),
            all(
                target_os = "macos",
                any(target_arch = "x86_64", target_arch = "aarch64")
            ),
            all(
                target_os = "linux",
                any(target_arch = "x86_64", target_arch = "aarch64")
            )
        )))]
        {
            assert!(patterns.is_empty());
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadResult {
    pub success: bool,
    pub message: String,
    #[serde(rename = "filePath")]
    pub file_path: Option<String>,
}

/// 下载更新安装包
///
/// 从 GitHub Releases 下载对应平台的安装包到下载目录
#[tauri::command]
pub async fn download_update(app_handle: AppHandle) -> Result<DownloadResult, String> {
    // 首先检查是否有更新
    let version_info = check_for_updates().await?;

    if !version_info.has_update {
        return Ok(DownloadResult {
            success: false,
            message: "当前已是最新版本".to_string(),
            file_path: None,
        });
    }

    let latest_version = version_info.latest.ok_or("无法获取最新版本信息")?;

    // 从 GitHub API 获取实际的文件列表并匹配平台
    let (filename, download_url) = get_platform_download_from_github(&latest_version).await?;

    // 获取下载目录
    let download_dir = get_download_directory(&app_handle)?;
    let file_path = download_dir.join(&filename);

    // 如果文件已存在，先删除
    if file_path.exists() {
        if let Err(e) = std::fs::remove_file(&file_path) {
            tracing::warn!("删除旧文件失败: {}", e);
        }
    }

    // 下载文件
    let client = reqwest::Client::new();

    match client
        .get(&download_url)
        .header("User-Agent", "ProxyCast")
        .send()
        .await
    {
        Ok(response) => {
            if !response.status().is_success() {
                return Ok(DownloadResult {
                    success: false,
                    message: format!("下载失败: HTTP {}", response.status()),
                    file_path: None,
                });
            }

            // 获取文件内容
            match response.bytes().await {
                Ok(bytes) => {
                    // 写入文件
                    match std::fs::write(&file_path, bytes) {
                        Ok(_) => {
                            tracing::info!("安装包下载成功: {:?}", file_path);

                            // 尝试直接运行安装程序
                            match run_installer(&file_path) {
                                Ok(_) => {
                                    tracing::info!("已启动安装程序，准备退出当前应用");

                                    // 延迟退出，给安装程序时间启动
                                    tokio::spawn(async {
                                        tokio::time::sleep(tokio::time::Duration::from_secs(2))
                                            .await;
                                        tracing::info!("自动退出应用以便安装程序运行");
                                        std::process::exit(0);
                                    });
                                }
                                Err(e) => {
                                    tracing::warn!("启动安装程序失败: {}，尝试打开文件位置", e);
                                    // 如果无法运行安装程序，则打开文件所在目录
                                    if let Err(open_err) = open_file_location(&file_path) {
                                        tracing::warn!("打开文件所在目录也失败: {}", open_err);
                                    }
                                }
                            }

                            Ok(DownloadResult {
                                success: true,
                                message: format!("下载完成: {}", filename),
                                file_path: Some(file_path.to_string_lossy().to_string()),
                            })
                        }
                        Err(e) => Ok(DownloadResult {
                            success: false,
                            message: format!("保存文件失败: {}", e),
                            file_path: None,
                        }),
                    }
                }
                Err(e) => Ok(DownloadResult {
                    success: false,
                    message: format!("读取下载内容失败: {}", e),
                    file_path: None,
                }),
            }
        }
        Err(e) => Ok(DownloadResult {
            success: false,
            message: format!("网络请求失败: {}", e),
            file_path: None,
        }),
    }
}

/// 从 GitHub API 获取实际的文件列表并匹配平台
async fn get_platform_download_from_github(version: &str) -> Result<(String, String), String> {
    let api_url = format!(
        "https://api.github.com/repos/aiclientproxy/proxycast/releases/tags/v{}",
        version
    );

    let client = reqwest::Client::new();
    let response = client
        .get(&api_url)
        .header("User-Agent", "ProxyCast")
        .send()
        .await
        .map_err(|e| format!("请求 GitHub API 失败: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("GitHub API 请求失败: {}", response.status()));
    }

    let data: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("解析 GitHub API 响应失败: {}", e))?;

    let assets = data["assets"]
        .as_array()
        .ok_or("GitHub API 响应中没有找到 assets")?;

    // 根据当前平台匹配文件
    let platform_patterns = get_platform_patterns();

    for asset in assets {
        let name = asset["name"].as_str().unwrap_or("");
        let download_url = asset["browser_download_url"].as_str().unwrap_or("");

        for pattern in &platform_patterns {
            if name.contains(pattern) {
                return Ok((name.to_string(), download_url.to_string()));
            }
        }
    }

    Err("未找到适合当前平台的安装包".to_string())
}

/// 获取当前平台的文件名匹配模式
fn get_platform_patterns() -> Vec<&'static str> {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        vec!["x64-setup.exe", "x64_en-US.msi"]
    }

    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        vec!["arm64-setup.exe", "arm64_en-US.msi"]
    }

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        vec!["x64.dmg"]
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        vec!["aarch64.dmg"]
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        vec!["amd64.deb", "amd64.AppImage"]
    }

    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        vec!["arm64.deb", "arm64.AppImage"]
    }

    #[cfg(not(any(
        all(
            target_os = "windows",
            any(target_arch = "x86_64", target_arch = "aarch64")
        ),
        all(
            target_os = "macos",
            any(target_arch = "x86_64", target_arch = "aarch64")
        ),
        all(
            target_os = "linux",
            any(target_arch = "x86_64", target_arch = "aarch64")
        )
    )))]
    {
        vec![]
    }
}

/// 获取下载目录
fn get_download_directory(app_handle: &AppHandle) -> Result<PathBuf, String> {
    // 优先使用系统下载目录
    if let Some(download_dir) = dirs::download_dir() {
        return Ok(download_dir);
    }

    // 回退到应用数据目录
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法获取应用数据目录: {}", e))?;

    let download_dir = app_data_dir.join("downloads");

    // 确保目录存在
    std::fs::create_dir_all(&download_dir).map_err(|e| format!("创建下载目录失败: {}", e))?;

    Ok(download_dir)
}

/// 运行安装程序
fn run_installer(file_path: &PathBuf) -> Result<(), String> {
    let extension = file_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");

    match extension.to_lowercase().as_str() {
        "exe" | "msi" => {
            #[cfg(target_os = "windows")]
            {
                tracing::info!("Windows: 启动安装程序: {:?}", file_path);
                std::process::Command::new(file_path)
                    .spawn()
                    .map_err(|e| format!("启动 Windows 安装程序失败: {}", e))?;
            }

            #[cfg(not(target_os = "windows"))]
            {
                return Err("Windows 安装程序只能在 Windows 系统上运行".to_string());
            }
        }
        "dmg" => {
            #[cfg(target_os = "macos")]
            {
                tracing::info!("macOS: 打开 DMG 文件: {:?}", file_path);
                std::process::Command::new("open")
                    .arg(&file_path)
                    .spawn()
                    .map_err(|e| format!("打开 macOS DMG 文件失败: {}", e))?;
            }

            #[cfg(not(target_os = "macos"))]
            {
                return Err("DMG 文件只能在 macOS 系统上打开".to_string());
            }
        }
        "deb" => {
            #[cfg(target_os = "linux")]
            {
                tracing::info!("Linux: 尝试安装 DEB 包: {:?}", file_path);
                // 尝试使用系统默认的包管理器打开
                let result = std::process::Command::new("xdg-open")
                    .arg(&file_path)
                    .spawn();

                if result.is_err() {
                    // 如果 xdg-open 失败，尝试使用 dpkg
                    tracing::info!("xdg-open 失败，尝试使用 gdebi 或提示用户手动安装");
                    return Err("请手动安装 DEB 包，或使用: sudo dpkg -i filename.deb".to_string());
                }
            }

            #[cfg(not(target_os = "linux"))]
            {
                return Err("DEB 包只能在 Linux 系统上安装".to_string());
            }
        }
        "appimage" => {
            #[cfg(target_os = "linux")]
            {
                tracing::info!("Linux: 设置 AppImage 可执行权限并运行: {:?}", file_path);
                // 设置可执行权限
                std::process::Command::new("chmod")
                    .args(&["+x", &file_path.to_string_lossy()])
                    .output()
                    .map_err(|e| format!("设置 AppImage 可执行权限失败: {}", e))?;

                // 运行 AppImage
                std::process::Command::new(&file_path)
                    .spawn()
                    .map_err(|e| format!("运行 AppImage 失败: {}", e))?;
            }

            #[cfg(not(target_os = "linux"))]
            {
                return Err("AppImage 只能在 Linux 系统上运行".to_string());
            }
        }
        _ => {
            return Err(format!("不支持的文件类型: {}", extension));
        }
    }

    Ok(())
}

/// 打开文件所在位置
fn open_file_location(file_path: &PathBuf) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        tracing::info!("Windows: 使用 explorer 打开文件位置: {:?}", file_path);
        std::process::Command::new("explorer")
            .args(["/select,", &file_path.to_string_lossy()])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .spawn()
            .map_err(|e| format!("Windows explorer 启动失败: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        tracing::info!("macOS: 使用 open -R 打开文件位置: {:?}", file_path);
        std::process::Command::new("open")
            .args(&["-R", &file_path.to_string_lossy()])
            .spawn()
            .map_err(|e| format!("macOS open 命令失败: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(parent) = file_path.parent() {
            tracing::info!("Linux: 使用 xdg-open 打开目录: {:?}", parent);
            std::process::Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|e| format!("Linux xdg-open 命令失败: {}", e))?;
        } else {
            return Err("无法获取文件的父目录".to_string());
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        return Err("不支持的操作系统".to_string());
    }

    Ok(())
}
