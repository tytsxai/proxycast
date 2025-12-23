import { useState, useEffect } from "react";
import { Check, AlertTriangle, Monitor } from "lucide-react";
import {
  getEndpointProviders,
  setEndpointProvider,
  EndpointProvidersConfig,
} from "@/hooks/useTauri";
import { providerPoolApi, ProviderPoolOverview } from "@/lib/api/providerPool";

const clientTypes = [
  { id: "cursor", label: "Cursor", description: "Cursor 编辑器" },
  {
    id: "claude_code",
    label: "Claude Code",
    description: "Claude Code 客户端",
  },
  { id: "codex", label: "Codex", description: "OpenAI Codex CLI" },
  { id: "windsurf", label: "Windsurf", description: "Windsurf 编辑器" },
  { id: "kiro", label: "Kiro", description: "Kiro IDE" },
  { id: "other", label: "其他", description: "未识别的客户端" },
] as const;

const providers = [
  { id: "kiro", label: "Kiro" },
  { id: "gemini", label: "Gemini" },
  { id: "qwen", label: "Qwen" },
  { id: "antigravity", label: "Antigravity" },
  { id: "openai", label: "OpenAI" },
  { id: "claude", label: "Claude" },
] as const;

interface ClientRoutingProps {
  loading?: boolean;
}

export function ClientRouting({
  loading: externalLoading,
}: ClientRoutingProps) {
  const [endpointProviders, setEndpointProviders] =
    useState<EndpointProvidersConfig>({});
  const [poolOverview, setPoolOverview] = useState<ProviderPoolOverview[]>([]);
  const [saveMsg, setSaveMsg] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  // 加载数据
  const loadData = async () => {
    setLoading(true);
    try {
      const [config, overview] = await Promise.all([
        getEndpointProviders(),
        providerPoolApi.getOverview(),
      ]);
      setEndpointProviders(config);
      setPoolOverview(overview);
    } catch (e) {
      console.error("Failed to load client routing config:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadData();
  }, []);

  // 自动清除保存消息
  useEffect(() => {
    if (saveMsg) {
      const timer = setTimeout(() => setSaveMsg(null), 3000);
      return () => clearTimeout(timer);
    }
  }, [saveMsg]);

  // 处理配置变更
  const handleSetProvider = async (
    clientType: string,
    provider: string | null,
  ) => {
    try {
      await setEndpointProvider(clientType, provider);
      setEndpointProviders((prev) => ({
        ...prev,
        [clientType]: provider,
      }));
      const clientLabel =
        clientTypes.find((c) => c.id === clientType)?.label || clientType;
      const providerLabel = provider
        ? providers.find((p) => p.id === provider)?.label || provider
        : "默认 Provider";
      setSaveMsg(`${clientLabel} 已设置为 ${providerLabel}`);
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      setSaveMsg(`保存失败: ${errMsg}`);
    }
  };

  // 检查是否有任何自定义配置
  const hasCustomConfig = Object.values(endpointProviders).some((v) => v);

  const isLoading = loading || externalLoading;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-lg font-semibold flex items-center gap-2">
            <Monitor className="h-5 w-5" />
            客户端路由
          </h3>
          <p className="text-sm text-muted-foreground">
            根据客户端 User-Agent 自动选择不同的 Provider
          </p>
        </div>
        {hasCustomConfig && (
          <span className="text-xs text-muted-foreground bg-muted px-2 py-1 rounded">
            已配置 {Object.values(endpointProviders).filter((v) => v).length} 项
          </span>
        )}
      </div>

      {/* 说明 */}
      <div className="rounded-lg bg-muted/50 p-3 text-xs text-muted-foreground">
        <p className="font-medium mb-1">工作原理：</p>
        <ul className="list-disc list-inside space-y-0.5">
          <li>系统根据请求的 User-Agent 头识别客户端类型</li>
          <li>选择"默认"时，使用 API Server 中配置的默认 Provider</li>
          <li>此配置优先级高于模型路由规则</li>
        </ul>
      </div>

      {/* 保存消息 */}
      {saveMsg && (
        <div
          className={`flex items-center gap-2 rounded-lg border p-2 text-sm ${
            saveMsg.includes("失败")
              ? "border-red-500 bg-red-50 text-red-700 dark:bg-red-950/30"
              : "border-green-500 bg-green-50 text-green-700 dark:bg-green-950/30"
          }`}
        >
          {saveMsg.includes("失败") ? (
            <AlertTriangle className="h-4 w-4" />
          ) : (
            <Check className="h-4 w-4" />
          )}
          {saveMsg}
        </div>
      )}

      {/* 客户端配置列表 */}
      {isLoading ? (
        <div className="flex items-center justify-center py-8 text-muted-foreground">
          加载中...
        </div>
      ) : (
        <div className="space-y-2">
          {clientTypes.map((client) => {
            const currentProvider =
              endpointProviders[client.id as keyof EndpointProvidersConfig];
            // 检查配置的 Provider 是否有可用凭证
            const hasCredentials = currentProvider
              ? poolOverview.some(
                  (o) =>
                    o.provider_type === currentProvider && o.stats.total > 0,
                )
              : true;

            return (
              <div
                key={client.id}
                className="flex items-center justify-between rounded-lg border bg-background p-3 hover:bg-muted/30 transition-colors"
              >
                <div className="flex-1">
                  <div className="flex items-center gap-2">
                    <span className="font-medium text-sm">{client.label}</span>
                    {!hasCredentials && currentProvider && (
                      <span className="flex items-center gap-1 text-xs text-amber-600 dark:text-amber-400">
                        <AlertTriangle className="h-3 w-3" />
                        无凭证
                      </span>
                    )}
                  </div>
                  <span className="text-xs text-muted-foreground">
                    {client.description}
                  </span>
                </div>
                <select
                  value={currentProvider || ""}
                  onChange={(e) =>
                    handleSetProvider(client.id, e.target.value || null)
                  }
                  className="rounded-lg border bg-background px-3 py-1.5 text-sm min-w-[160px] focus:border-primary focus:outline-none"
                >
                  <option value="">默认</option>
                  {providers.map((provider) => {
                    const overview = poolOverview.find(
                      (o) => o.provider_type === provider.id,
                    );
                    const count = overview?.stats.total || 0;
                    return (
                      <option key={provider.id} value={provider.id}>
                        {provider.label} {count > 0 ? `(${count})` : ""}
                      </option>
                    );
                  })}
                </select>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
