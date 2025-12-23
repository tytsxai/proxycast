import { useState } from "react";
import {
  Heart,
  HeartOff,
  Trash2,
  RotateCcw,
  Activity,
  Power,
  PowerOff,
  Clock,
  AlertTriangle,
  RefreshCw,
  Settings,
  Upload,
  Lock,
  User,
  Globe,
  BarChart3,
  ChevronUp,
  Fingerprint,
  Copy,
  Check,
  Timer,
} from "lucide-react";
import type {
  CredentialDisplay,
  CredentialSource,
} from "@/lib/api/providerPool";
import {
  getKiroCredentialFingerprint,
  type KiroFingerprintInfo,
} from "@/lib/api/providerPool";
import { usageApi, type UsageInfo } from "@/lib/api/usage";
import { UsageDisplay } from "./UsageDisplay";

interface CredentialCardProps {
  credential: CredentialDisplay;
  onToggle: () => void;
  onDelete: () => void;
  onReset: () => void;
  onCheckHealth: () => void;
  onRefreshToken?: () => void;
  onEdit: () => void;
  deleting: boolean;
  checkingHealth: boolean;
  refreshingToken?: boolean;
  /** 是否为 Kiro 凭证（支持用量查询） */
  isKiroCredential?: boolean;
}

export function CredentialCard({
  credential,
  onToggle,
  onDelete,
  onReset,
  onCheckHealth,
  onRefreshToken,
  onEdit,
  deleting,
  checkingHealth,
  refreshingToken,
  isKiroCredential,
}: CredentialCardProps) {
  // 用量查询状态
  const [usageExpanded, setUsageExpanded] = useState(false);
  const [usageLoading, setUsageLoading] = useState(false);
  const [usageInfo, setUsageInfo] = useState<UsageInfo | null>(null);
  const [usageError, setUsageError] = useState<string | null>(null);

  // 指纹信息状态（仅 Kiro 凭证）
  const [fingerprintInfo, setFingerprintInfo] =
    useState<KiroFingerprintInfo | null>(null);
  const [fingerprintLoading, setFingerprintLoading] = useState(false);
  const [fingerprintExpanded, setFingerprintExpanded] = useState(false);
  const [fingerprintCopied, setFingerprintCopied] = useState(false);

  // 查询指纹信息
  const handleCheckFingerprint = async () => {
    if (fingerprintExpanded && fingerprintInfo) {
      // 已展开且有数据，直接折叠
      setFingerprintExpanded(false);
      return;
    }

    setFingerprintExpanded(true);
    setFingerprintLoading(true);

    try {
      const info = await getKiroCredentialFingerprint(credential.uuid);
      setFingerprintInfo(info);
    } catch (e) {
      console.error("获取指纹信息失败:", e);
    } finally {
      setFingerprintLoading(false);
    }
  };

  // 复制 Machine ID
  const handleCopyMachineId = async () => {
    if (!fingerprintInfo) return;
    try {
      await navigator.clipboard.writeText(fingerprintInfo.machine_id);
      setFingerprintCopied(true);
      setTimeout(() => setFingerprintCopied(false), 2000);
    } catch (e) {
      console.error("复制失败:", e);
    }
  };

  // 查询用量
  const handleCheckUsage = async () => {
    if (usageExpanded && usageInfo) {
      // 已展开且有数据，直接折叠
      setUsageExpanded(false);
      return;
    }

    setUsageExpanded(true);
    setUsageLoading(true);
    setUsageError(null);

    try {
      const info = await usageApi.getKiroUsage(credential.uuid);
      setUsageInfo(info);
    } catch (e) {
      setUsageError(e instanceof Error ? e.message : String(e));
    } finally {
      setUsageLoading(false);
    }
  };

  const formatDate = (dateStr?: string) => {
    if (!dateStr) return "从未";
    const date = new Date(dateStr);
    return date.toLocaleString("zh-CN", {
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  };

  const getCredentialTypeLabel = (type: string) => {
    const labels: Record<string, string> = {
      kiro_oauth: "OAuth",
      gemini_oauth: "OAuth",
      qwen_oauth: "OAuth",
      antigravity_oauth: "OAuth",
      openai_key: "API Key",
      claude_key: "API Key",
      codex_oauth: "OAuth",
      claude_oauth: "OAuth",
      iflow_oauth: "OAuth",
      iflow_cookie: "Cookie",
    };
    return labels[type] || type;
  };

  const getSourceLabel = (source: CredentialSource) => {
    const labels: Record<
      CredentialSource,
      { text: string; icon: typeof User; color: string }
    > = {
      manual: {
        text: "手动添加",
        icon: User,
        color:
          "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400",
      },
      imported: {
        text: "导入",
        icon: Upload,
        color:
          "bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400",
      },
      private: {
        text: "私有",
        icon: Lock,
        color:
          "bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400",
      },
    };
    return labels[source] || labels.manual;
  };

  const sourceInfo = getSourceLabel(credential.source || "manual");
  const SourceIcon = sourceInfo.icon;

  const isHealthy = credential.is_healthy && !credential.is_disabled;
  const hasError = credential.error_count > 0;
  const isOAuth = credential.credential_type.includes("oauth");

  return (
    <div
      className={`rounded-xl border transition-all hover:shadow-md ${
        credential.is_disabled
          ? "border-gray-200 bg-gray-50/80 opacity-70 dark:border-gray-700 dark:bg-gray-900/60"
          : isHealthy
            ? "border-green-200 bg-gradient-to-r from-green-50/80 to-white dark:border-green-800 dark:bg-gradient-to-r dark:from-green-950/40 dark:to-transparent"
            : "border-red-200 bg-gradient-to-r from-red-50/80 to-white dark:border-red-800 dark:bg-gradient-to-r dark:from-red-950/40 dark:to-transparent"
      }`}
    >
      {/* 第一行：状态图标 + 名称 + 标签 + 操作按钮 */}
      <div className="flex items-center gap-4 p-4 pb-3">
        {/* Status Icon */}
        <div
          className={`shrink-0 rounded-full p-3 ${
            credential.is_disabled
              ? "bg-gray-100 dark:bg-gray-800"
              : isHealthy
                ? "bg-green-100 dark:bg-green-900/30"
                : "bg-red-100 dark:bg-red-900/30"
          }`}
        >
          {credential.is_disabled ? (
            <PowerOff className="h-6 w-6 text-gray-400" />
          ) : isHealthy ? (
            <Heart className="h-6 w-6 text-green-600 dark:text-green-400" />
          ) : (
            <HeartOff className="h-6 w-6 text-red-600 dark:text-red-400" />
          )}
        </div>

        {/* Main Info */}
        <div className="flex-1 min-w-0">
          <h4 className="font-semibold text-lg truncate">
            {credential.name || `凭证 #${credential.uuid.slice(0, 8)}`}
          </h4>
          <div className="flex flex-wrap items-center gap-2 mt-1.5">
            <span className="rounded-full bg-muted px-2.5 py-1 text-xs font-medium">
              {getCredentialTypeLabel(credential.credential_type)}
            </span>
            <span
              className={`rounded-full px-2.5 py-1 text-xs font-medium inline-flex items-center gap-1.5 whitespace-nowrap ${sourceInfo.color}`}
            >
              <SourceIcon className="h-3 w-3 shrink-0" />
              {sourceInfo.text}
            </span>
            {credential.proxy_url && (
              <span
                className="rounded-full px-2.5 py-1 text-xs font-medium inline-flex items-center gap-1.5 whitespace-nowrap bg-orange-100 text-orange-700 dark:bg-orange-900/30 dark:text-orange-400"
                title={`代理: ${credential.proxy_url}`}
              >
                <Globe className="h-3 w-3 shrink-0" />
                代理
              </span>
            )}
          </div>
        </div>

        {/* Actions */}
        <div className="flex items-center gap-2 shrink-0">
          <button
            onClick={onToggle}
            className={`rounded-lg p-2.5 text-xs font-medium transition-colors ${
              credential.is_disabled
                ? "bg-green-100 text-green-700 hover:bg-green-200 dark:bg-green-900/30 dark:text-green-400"
                : "bg-gray-100 text-gray-700 hover:bg-gray-200 dark:bg-gray-800 dark:text-gray-300"
            }`}
            title={credential.is_disabled ? "启用" : "禁用"}
          >
            {credential.is_disabled ? (
              <Power className="h-4 w-4" />
            ) : (
              <PowerOff className="h-4 w-4" />
            )}
          </button>

          <button
            onClick={onEdit}
            className="rounded-lg bg-blue-100 p-2.5 text-blue-700 hover:bg-blue-200 dark:bg-blue-900/30 dark:text-blue-400 transition-colors"
            title="编辑"
          >
            <Settings className="h-4 w-4" />
          </button>

          <button
            onClick={onCheckHealth}
            disabled={checkingHealth}
            className="rounded-lg bg-emerald-100 p-2.5 text-emerald-700 hover:bg-emerald-200 disabled:opacity-50 dark:bg-emerald-900/30 dark:text-emerald-400 transition-colors"
            title="检测"
          >
            <Activity
              className={`h-4 w-4 ${checkingHealth ? "animate-pulse" : ""}`}
            />
          </button>

          {isOAuth && onRefreshToken && (
            <button
              onClick={onRefreshToken}
              disabled={refreshingToken}
              className="rounded-lg bg-purple-100 p-2.5 text-purple-700 hover:bg-purple-200 disabled:opacity-50 dark:bg-purple-900/30 dark:text-purple-400 transition-colors"
              title="刷新 Token"
            >
              <RefreshCw
                className={`h-4 w-4 ${refreshingToken ? "animate-spin" : ""}`}
              />
            </button>
          )}

          {/* 指纹信息按钮 - 仅 Kiro 凭证显示 */}
          {isKiroCredential && (
            <button
              onClick={handleCheckFingerprint}
              disabled={fingerprintLoading}
              className={`rounded-lg p-2.5 transition-colors ${
                fingerprintExpanded
                  ? "bg-indigo-200 text-indigo-800 dark:bg-indigo-800 dark:text-indigo-200"
                  : "bg-indigo-100 text-indigo-700 hover:bg-indigo-200 dark:bg-indigo-900/30 dark:text-indigo-400"
              } disabled:opacity-50`}
              title="查看设备指纹"
            >
              <Fingerprint
                className={`h-4 w-4 ${fingerprintLoading ? "animate-pulse" : ""}`}
              />
            </button>
          )}

          {/* 用量查询按钮 - 仅 Kiro 凭证显示 */}
          {isKiroCredential && (
            <button
              onClick={handleCheckUsage}
              disabled={usageLoading}
              className={`rounded-lg p-2.5 transition-colors ${
                usageExpanded
                  ? "bg-cyan-200 text-cyan-800 dark:bg-cyan-800 dark:text-cyan-200"
                  : "bg-cyan-100 text-cyan-700 hover:bg-cyan-200 dark:bg-cyan-900/30 dark:text-cyan-400"
              } disabled:opacity-50`}
              title="查看用量"
            >
              <BarChart3
                className={`h-4 w-4 ${usageLoading ? "animate-pulse" : ""}`}
              />
            </button>
          )}

          <button
            onClick={onReset}
            className="rounded-lg bg-orange-100 p-2.5 text-orange-700 hover:bg-orange-200 dark:bg-orange-900/30 dark:text-orange-400 transition-colors"
            title="重置"
          >
            <RotateCcw className="h-4 w-4" />
          </button>

          <button
            onClick={onDelete}
            disabled={deleting}
            className="rounded-lg bg-red-100 p-2.5 text-red-700 hover:bg-red-200 disabled:opacity-50 dark:bg-red-900/30 dark:text-red-400 transition-colors"
            title="删除"
          >
            <Trash2 className="h-4 w-4" />
          </button>
        </div>
      </div>

      {/* 第二行：统计信息 - 使用网格布局 */}
      <div className="hidden sm:block px-4 py-3 bg-muted/30 border-t border-border/30">
        <div className="grid grid-cols-5 gap-4">
          {/* 使用次数 */}
          <div className="flex items-center gap-3">
            <Activity className="h-5 w-5 text-blue-500 shrink-0" />
            <div>
              <div className="text-xs text-muted-foreground">使用次数</div>
              <div className="font-bold text-xl tabular-nums">
                {credential.usage_count}
              </div>
            </div>
          </div>

          {/* 错误次数 */}
          <div className="flex items-center gap-3">
            <AlertTriangle
              className={`h-5 w-5 shrink-0 ${hasError ? "text-yellow-500" : "text-green-500"}`}
            />
            <div>
              <div className="text-xs text-muted-foreground">错误次数</div>
              <div className="font-bold text-xl tabular-nums">
                {credential.error_count}
              </div>
            </div>
          </div>

          {/* 最后使用 */}
          <div className="flex items-center gap-3">
            <Clock className="h-5 w-5 text-muted-foreground shrink-0" />
            <div>
              <div className="text-xs text-muted-foreground">最后使用</div>
              <div className="font-medium text-sm">
                {formatDate(credential.last_used)}
              </div>
            </div>
          </div>

          {/* Token 有效期 - OAuth 凭证显示 */}
          {isOAuth ? (
            <div className="flex items-center gap-3">
              <Timer
                className={`h-5 w-5 shrink-0 ${
                  credential.token_cache_status?.expiry_time
                    ? credential.token_cache_status.is_expiring_soon
                      ? "text-yellow-500"
                      : credential.token_cache_status.is_valid
                        ? "text-green-500"
                        : "text-red-500"
                    : "text-gray-400"
                }`}
              />
              <div>
                <div className="text-xs text-muted-foreground">
                  Token 有效期
                </div>
                {credential.token_cache_status?.expiry_time ? (
                  <div
                    className={`font-medium text-sm ${
                      credential.token_cache_status.is_expiring_soon
                        ? "text-yellow-600 dark:text-yellow-400"
                        : credential.token_cache_status.is_valid
                          ? "text-green-600 dark:text-green-400"
                          : "text-red-600 dark:text-red-400"
                    }`}
                  >
                    {formatDate(credential.token_cache_status.expiry_time)}
                  </div>
                ) : (
                  <div className="text-sm text-muted-foreground">--</div>
                )}
              </div>
            </div>
          ) : (
            <div /> /* 占位 */
          )}

          {/* 健康检查 */}
          {credential.last_health_check_time ? (
            <div className="flex items-center gap-3">
              <Activity className="h-5 w-5 text-emerald-500 shrink-0" />
              <div>
                <div className="text-xs text-muted-foreground">健康检查</div>
                <div className="font-medium text-sm">
                  {formatDate(credential.last_health_check_time)}
                </div>
              </div>
            </div>
          ) : (
            <div /> /* 占位 */
          )}
        </div>
      </div>

      {/* 第三行：UUID */}
      <div className="px-4 py-2 border-t border-border/30">
        <p className="text-xs text-muted-foreground font-mono">
          {credential.uuid}
        </p>
      </div>

      {/* Mobile Stats - shown on small screens */}
      <div className="sm:hidden px-4 py-3 bg-muted/30 border-t border-border/30">
        <div className="grid grid-cols-2 gap-4">
          <div className="flex items-center gap-2">
            <Activity className="h-4 w-4 text-blue-500" />
            <span className="text-xs text-muted-foreground">使用:</span>
            <span className="font-semibold">{credential.usage_count}</span>
          </div>
          <div className="flex items-center gap-2">
            <AlertTriangle
              className={`h-4 w-4 ${hasError ? "text-yellow-500" : "text-green-500"}`}
            />
            <span className="text-xs text-muted-foreground">错误:</span>
            <span className="font-semibold">{credential.error_count}</span>
          </div>
          <div className="flex items-center gap-2 col-span-2">
            <Clock className="h-4 w-4 text-muted-foreground" />
            <span className="text-xs text-muted-foreground">最后使用:</span>
            <span className="text-sm">{formatDate(credential.last_used)}</span>
          </div>
        </div>
      </div>

      {/* Error Message */}
      {credential.last_error_message && (
        <div className="mx-4 mb-3 rounded-lg bg-red-100 p-3 text-xs text-red-700 dark:bg-red-900/30 dark:text-red-300">
          {credential.last_error_message.slice(0, 150)}
          {credential.last_error_message.length > 150 && "..."}
        </div>
      )}

      {/* 指纹信息展示区域 - 仅 Kiro 凭证 */}
      {isKiroCredential && fingerprintExpanded && (
        <div className="mx-4 mb-4 p-4 rounded-lg bg-indigo-50 dark:bg-indigo-950/30 border border-indigo-200 dark:border-indigo-800">
          <div className="flex items-center justify-between mb-3">
            <span className="text-sm font-medium text-indigo-700 dark:text-indigo-300 flex items-center gap-2">
              <Fingerprint className="h-4 w-4" />
              设备指纹
            </span>
            <button
              onClick={() => setFingerprintExpanded(false)}
              className="text-indigo-500 hover:text-indigo-700 dark:hover:text-indigo-300"
            >
              <ChevronUp className="h-4 w-4" />
            </button>
          </div>

          {fingerprintLoading ? (
            <div className="flex items-center gap-2 text-sm text-indigo-600 dark:text-indigo-400">
              <div className="animate-spin h-4 w-4 border-2 border-current border-t-transparent rounded-full" />
              加载中...
            </div>
          ) : fingerprintInfo ? (
            <div className="space-y-3">
              <div className="flex items-center gap-2">
                <span className="text-sm text-muted-foreground">
                  Machine ID:
                </span>
                <code className="text-sm font-mono bg-white dark:bg-gray-800 px-2 py-1 rounded border">
                  {fingerprintInfo.machine_id_short}...
                </code>
                <button
                  onClick={handleCopyMachineId}
                  className="p-1.5 rounded hover:bg-indigo-100 dark:hover:bg-indigo-900/50 transition-colors"
                  title="复制完整 Machine ID"
                >
                  {fingerprintCopied ? (
                    <Check className="h-4 w-4 text-green-500" />
                  ) : (
                    <Copy className="h-4 w-4 text-muted-foreground" />
                  )}
                </button>
              </div>
              <div className="flex items-center gap-6 text-sm">
                <span className="flex items-center gap-2">
                  <span className="text-muted-foreground">来源:</span>
                  <span
                    className={`px-2 py-0.5 rounded font-medium ${
                      fingerprintInfo.source === "profileArn"
                        ? "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400"
                        : fingerprintInfo.source === "clientId"
                          ? "bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400"
                          : "bg-gray-100 text-gray-700 dark:bg-gray-800 dark:text-gray-400"
                    }`}
                  >
                    {fingerprintInfo.source}
                  </span>
                </span>
                <span className="flex items-center gap-2">
                  <span className="text-muted-foreground">认证:</span>
                  <span
                    className={`px-2 py-0.5 rounded font-medium ${
                      fingerprintInfo.auth_method.toLowerCase() === "idc"
                        ? "bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400"
                        : "bg-orange-100 text-orange-700 dark:bg-orange-900/30 dark:text-orange-400"
                    }`}
                  >
                    {fingerprintInfo.auth_method}
                  </span>
                </span>
              </div>
            </div>
          ) : (
            <div className="text-sm text-muted-foreground">
              无法获取指纹信息
            </div>
          )}
        </div>
      )}

      {/* 用量信息展示区域 - 仅 Kiro 凭证 */}
      {isKiroCredential && usageExpanded && (
        <div className="mx-4 mb-4 p-4 rounded-lg bg-cyan-50 dark:bg-cyan-950/30 border border-cyan-200 dark:border-cyan-800">
          <div className="flex items-center justify-between mb-3">
            <span className="text-sm font-medium text-cyan-700 dark:text-cyan-300 flex items-center gap-2">
              <BarChart3 className="h-4 w-4" />
              Kiro 用量
            </span>
            <button
              onClick={() => setUsageExpanded(false)}
              className="text-cyan-500 hover:text-cyan-700 dark:hover:text-cyan-300"
            >
              <ChevronUp className="h-4 w-4" />
            </button>
          </div>

          {usageError ? (
            <div className="rounded-lg bg-red-100 p-3 text-sm text-red-700 dark:bg-red-900/30 dark:text-red-300">
              {usageError}
            </div>
          ) : usageInfo ? (
            <UsageDisplay usage={usageInfo} loading={usageLoading} />
          ) : (
            <UsageDisplay
              usage={{
                subscriptionTitle: "",
                usageLimit: 0,
                currentUsage: 0,
                balance: 0,
                isLowBalance: false,
              }}
              loading={true}
            />
          )}
        </div>
      )}
    </div>
  );
}
