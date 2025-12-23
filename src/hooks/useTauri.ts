import { invoke } from "@tauri-apps/api/core";

export interface ServerStatus {
  running: boolean;
  host: string;
  port: number;
  requests: number;
  uptime_secs: number;
}

// TLS Configuration
export interface TlsConfig {
  enable: boolean;
  cert_path: string | null;
  key_path: string | null;
}

// Remote Management Configuration
export interface RemoteManagementConfig {
  allow_remote: boolean;
  secret_key: string | null;
  disable_control_panel: boolean;
}

// Quota Exceeded Configuration
export interface QuotaExceededConfig {
  switch_project: boolean;
  switch_preview_model: boolean;
  cooldown_seconds: number;
}

// Amp Model Mapping
export interface AmpModelMapping {
  from: string;
  to: string;
}

// Amp CLI Configuration
export interface AmpConfig {
  upstream_url: string | null;
  model_mappings: AmpModelMapping[];
  restrict_management_to_localhost: boolean;
}

// Gemini API Key Entry
export interface GeminiApiKeyEntry {
  id: string;
  api_key: string;
  base_url: string | null;
  proxy_url: string | null;
  excluded_models: string[];
  disabled: boolean;
}

// Vertex Model Alias
export interface VertexModelAlias {
  name: string;
  alias: string;
}

// Vertex API Key Entry
export interface VertexApiKeyEntry {
  id: string;
  api_key: string;
  base_url: string | null;
  models: VertexModelAlias[];
  proxy_url: string | null;
  disabled: boolean;
}

// iFlow Credential Entry
export interface IFlowCredentialEntry {
  id: string;
  token_file: string | null;
  auth_type: string;
  cookies: string | null;
  proxy_url: string | null;
  disabled: boolean;
}

// Credential Entry (OAuth)
export interface CredentialEntry {
  id: string;
  token_file: string;
  disabled: boolean;
  proxy_url: string | null;
}

// Credential Pool Configuration
export interface CredentialPoolConfig {
  kiro: CredentialEntry[];
  gemini: CredentialEntry[];
  qwen: CredentialEntry[];
  openai: ApiKeyEntry[];
  claude: ApiKeyEntry[];
  gemini_api_keys: GeminiApiKeyEntry[];
  vertex_api_keys: VertexApiKeyEntry[];
  codex: CredentialEntry[];
  iflow: IFlowCredentialEntry[];
}

// API Key Entry
export interface ApiKeyEntry {
  id: string;
  api_key: string;
  base_url: string | null;
  disabled: boolean;
  proxy_url: string | null;
}

export interface Config {
  server: {
    host: string;
    port: number;
    api_key: string;
    tls: TlsConfig;
  };
  providers: {
    kiro: {
      enabled: boolean;
      credentials_path: string | null;
      region: string | null;
    };
    gemini: {
      enabled: boolean;
      credentials_path: string | null;
    };
    qwen: {
      enabled: boolean;
      credentials_path: string | null;
    };
    openai: {
      enabled: boolean;
      api_key: string | null;
      base_url: string | null;
    };
    claude: {
      enabled: boolean;
      api_key: string | null;
      base_url: string | null;
    };
  };
  default_provider: string;
  remote_management: RemoteManagementConfig;
  quota_exceeded: QuotaExceededConfig;
  ampcode: AmpConfig;
  credential_pool: CredentialPoolConfig;
  proxy_url: string | null;
  /** 关闭时最小化到托盘（而不是退出应用） */
  minimize_to_tray: boolean;
}

export interface LogEntry {
  timestamp: string;
  level: string;
  message: string;
}

export async function startServer(): Promise<string> {
  return invoke("start_server");
}

export async function stopServer(): Promise<string> {
  return invoke("stop_server");
}

export async function getServerStatus(): Promise<ServerStatus> {
  return invoke("get_server_status");
}

export async function getConfig(): Promise<Config> {
  return invoke("get_config");
}

export async function saveConfig(config: Config): Promise<void> {
  return invoke("save_config", { config });
}

export async function getDefaultProvider(): Promise<string> {
  return invoke("get_default_provider");
}

export async function setDefaultProvider(provider: string): Promise<string> {
  return invoke("set_default_provider", { provider });
}

export async function refreshKiroToken(): Promise<string> {
  return invoke("refresh_kiro_token");
}

export async function reloadCredentials(): Promise<string> {
  return invoke("reload_credentials");
}

export async function getLogs(): Promise<LogEntry[]> {
  try {
    return await invoke("get_logs");
  } catch {
    return [];
  }
}

export async function clearLogs(): Promise<void> {
  try {
    await invoke("clear_logs");
  } catch {
    // ignore
  }
}

export interface TestResult {
  success: boolean;
  status: number;
  body: string;
  time_ms: number;
}

export async function testApi(
  method: string,
  path: string,
  body: string | null,
  auth: boolean,
): Promise<TestResult> {
  return invoke("test_api", { method, path, body, auth });
}

export interface KiroCredentialStatus {
  loaded: boolean;
  has_access_token: boolean;
  has_refresh_token: boolean;
  region: string | null;
  auth_method: string | null;
  expires_at: string | null;
  creds_path: string;
}

export async function getKiroCredentials(): Promise<KiroCredentialStatus> {
  return invoke("get_kiro_credentials");
}

export interface EnvVariable {
  key: string;
  value: string;
  masked: string;
}

export async function getEnvVariables(): Promise<EnvVariable[]> {
  return invoke("get_env_variables");
}

export async function getTokenFileHash(): Promise<string> {
  return invoke("get_token_file_hash");
}

export interface CheckResult {
  changed: boolean;
  new_hash: string;
  reloaded: boolean;
}

export async function checkAndReloadCredentials(
  lastHash: string,
): Promise<CheckResult> {
  return invoke("check_and_reload_credentials", { last_hash: lastHash });
}

// ============ Gemini Provider ============

export interface GeminiCredentialStatus {
  loaded: boolean;
  has_access_token: boolean;
  has_refresh_token: boolean;
  expiry_date: number | null;
  is_valid: boolean;
  creds_path: string;
}

export async function getGeminiCredentials(): Promise<GeminiCredentialStatus> {
  return invoke("get_gemini_credentials");
}

export async function reloadGeminiCredentials(): Promise<string> {
  return invoke("reload_gemini_credentials");
}

export async function refreshGeminiToken(): Promise<string> {
  return invoke("refresh_gemini_token");
}

export async function getGeminiEnvVariables(): Promise<EnvVariable[]> {
  return invoke("get_gemini_env_variables");
}

export async function getGeminiTokenFileHash(): Promise<string> {
  return invoke("get_gemini_token_file_hash");
}

export async function checkAndReloadGeminiCredentials(
  lastHash: string,
): Promise<CheckResult> {
  return invoke("check_and_reload_gemini_credentials", { last_hash: lastHash });
}

// ============ Qwen Provider ============

export interface QwenCredentialStatus {
  loaded: boolean;
  has_access_token: boolean;
  has_refresh_token: boolean;
  expiry_date: number | null;
  is_valid: boolean;
  creds_path: string;
}

export async function getQwenCredentials(): Promise<QwenCredentialStatus> {
  return invoke("get_qwen_credentials");
}

export async function reloadQwenCredentials(): Promise<string> {
  return invoke("reload_qwen_credentials");
}

export async function refreshQwenToken(): Promise<string> {
  return invoke("refresh_qwen_token");
}

export async function getQwenEnvVariables(): Promise<EnvVariable[]> {
  return invoke("get_qwen_env_variables");
}

export async function getQwenTokenFileHash(): Promise<string> {
  return invoke("get_qwen_token_file_hash");
}

export async function checkAndReloadQwenCredentials(
  lastHash: string,
): Promise<CheckResult> {
  return invoke("check_and_reload_qwen_credentials", { last_hash: lastHash });
}

// ============ OpenAI Custom Provider ============

export interface OpenAICustomStatus {
  enabled: boolean;
  has_api_key: boolean;
  base_url: string;
}

export async function getOpenAICustomStatus(): Promise<OpenAICustomStatus> {
  return invoke("get_openai_custom_status");
}

export async function setOpenAICustomConfig(
  apiKey: string | null,
  baseUrl: string | null,
  enabled: boolean,
): Promise<string> {
  return invoke("set_openai_custom_config", {
    api_key: apiKey,
    base_url: baseUrl,
    enabled,
  });
}

// ============ Claude Custom Provider ============

export interface ClaudeCustomStatus {
  enabled: boolean;
  has_api_key: boolean;
  base_url: string;
}

export async function getClaudeCustomStatus(): Promise<ClaudeCustomStatus> {
  return invoke("get_claude_custom_status");
}

export async function setClaudeCustomConfig(
  apiKey: string | null,
  baseUrl: string | null,
  enabled: boolean,
): Promise<string> {
  return invoke("set_claude_custom_config", {
    api_key: apiKey,
    base_url: baseUrl,
    enabled,
  });
}

// ============ Models ============

export interface ModelInfo {
  id: string;
  object: string;
  owned_by: string;
}

export async function getAvailableModels(): Promise<ModelInfo[]> {
  return invoke("get_available_models");
}

// ============ API Compatibility Check ============

export interface ApiCheckResult {
  model: string;
  available: boolean;
  status: number;
  error_type: string | null;
  error_message: string | null;
  time_ms: number;
}

export interface ApiCompatibilityResult {
  provider: string;
  overall_status: string;
  checked_at: string;
  results: ApiCheckResult[];
  warnings: string[];
}

export async function checkApiCompatibility(
  provider: string,
): Promise<ApiCompatibilityResult> {
  return invoke("check_api_compatibility", { provider });
}

// ============ Endpoint Provider Configuration ============

/**
 * 端点 Provider 配置
 * 为不同客户端类型配置不同的 LLM Provider
 */
export interface EndpointProvidersConfig {
  /** Cursor 客户端使用的 Provider */
  cursor?: string | null;
  /** Claude Code 客户端使用的 Provider */
  claude_code?: string | null;
  /** Codex 客户端使用的 Provider */
  codex?: string | null;
  /** Windsurf 客户端使用的 Provider */
  windsurf?: string | null;
  /** Kiro 客户端使用的 Provider */
  kiro?: string | null;
  /** 其他客户端使用的 Provider */
  other?: string | null;
}

/**
 * 获取端点 Provider 配置
 * @returns 端点 Provider 配置对象
 */
export async function getEndpointProviders(): Promise<EndpointProvidersConfig> {
  return invoke("get_endpoint_providers");
}

/**
 * 设置端点 Provider 配置
 * @param clientType 客户端类型 (cursor, claude_code, codex, windsurf, kiro, other)
 * @param provider Provider 名称，传 null 表示使用默认 Provider
 * @returns 设置后的 Provider 名称
 */
export async function setEndpointProvider(
  clientType: string,
  provider: string | null,
): Promise<string> {
  return invoke("set_endpoint_provider", { endpoint: clientType, provider });
}
