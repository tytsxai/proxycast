import { useState, useEffect, forwardRef, useImperativeHandle } from "react";
import { RefreshCw, Route, Sparkles, Check, Trash2 } from "lucide-react";
import { ModelMapping } from "./ModelMapping";
import { RoutingRules } from "./RoutingRules";
import { ExclusionList } from "./ExclusionList";
import { InjectionRules } from "./InjectionRules";
import { ClientRouting } from "./ClientRouting";
import { HelpTip } from "@/components/HelpTip";
import { routerApi } from "@/lib/api/router";
import { injectionApi } from "@/lib/api/injection";
import { setEndpointProvider } from "@/hooks/useTauri";
import type {
  ModelAlias,
  RoutingRule,
  ProviderType,
  RecommendedPreset,
} from "@/lib/api/router";
import type { InjectionRule } from "@/lib/api/injection";

export interface RoutingPageRef {
  refresh: () => void;
}

type TabType = "aliases" | "rules" | "exclusions" | "injection" | "clients";

interface RoutingPageProps {
  hideHeader?: boolean;
}

export const RoutingPage = forwardRef<RoutingPageRef, RoutingPageProps>(
  ({ hideHeader = false }, ref) => {
    const [activeTab, setActiveTab] = useState<TabType>("clients");
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    // Data state
    const [aliases, setAliases] = useState<ModelAlias[]>([]);
    const [rules, setRules] = useState<RoutingRule[]>([]);
    const [exclusions, setExclusions] = useState<
      Record<ProviderType, string[]>
    >({} as Record<ProviderType, string[]>);
    const [injectionRules, setInjectionRules] = useState<InjectionRule[]>([]);
    const [injectionEnabled, setInjectionEnabled] = useState(false);

    // Presets state
    const [presets, setPresets] = useState<RecommendedPreset[]>([]);
    const [showPresets, setShowPresets] = useState(false);
    const [applyingPreset, setApplyingPreset] = useState<string | null>(null);

    const refresh = async () => {
      setLoading(true);
      setError(null);
      try {
        const [
          aliasesData,
          rulesData,
          exclusionsData,
          injectionConfig,
          presetsData,
        ] = await Promise.all([
          routerApi.getModelAliases(),
          routerApi.getRoutingRules(),
          routerApi.getExclusions(),
          injectionApi.getInjectionConfig(),
          routerApi.getRecommendedPresets(),
        ]);
        setAliases(aliasesData);
        setRules(rulesData);
        setExclusions(exclusionsData);
        setInjectionRules(injectionConfig.rules);
        setInjectionEnabled(injectionConfig.enabled);
        setPresets(presetsData);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setLoading(false);
      }
    };

    const handleApplyPreset = async (
      presetId: string,
      merge: boolean = false,
    ) => {
      setApplyingPreset(presetId);
      try {
        // 先找到预设配置
        const preset = presets.find((p) => p.id === presetId);

        // 应用别名和规则
        await routerApi.applyRecommendedPreset(presetId, merge);

        // 如果预设包含客户端路由配置，也应用它
        if (preset?.endpoint_providers) {
          const ep = preset.endpoint_providers;
          const clientTypes = [
            "cursor",
            "claude_code",
            "codex",
            "windsurf",
            "kiro",
            "other",
          ] as const;
          for (const clientType of clientTypes) {
            const provider = ep[clientType];
            // 只有在非合并模式或有值时才设置
            if (!merge || provider) {
              await setEndpointProvider(clientType, provider || null);
            }
          }
        }

        await refresh();
        setShowPresets(false);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setApplyingPreset(null);
      }
    };

    const handleClearAll = async () => {
      if (!confirm("确定要清空所有路由配置吗？此操作不可撤销。")) return;
      try {
        await routerApi.clearAllRoutingConfig();
        await refresh();
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      }
    };

    useImperativeHandle(ref, () => ({
      refresh,
    }));

    useEffect(() => {
      refresh();
    }, []);

    // Alias handlers
    const handleAddAlias = async (alias: string, actual: string) => {
      await routerApi.addModelAlias(alias, actual);
      await refresh();
    };

    const handleRemoveAlias = async (alias: string) => {
      await routerApi.removeModelAlias(alias);
      await refresh();
    };

    // Rule handlers
    const handleAddRule = async (rule: RoutingRule) => {
      await routerApi.addRoutingRule(rule);
      await refresh();
    };

    const handleRemoveRule = async (pattern: string) => {
      await routerApi.removeRoutingRule(pattern);
      await refresh();
    };

    const handleUpdateRule = async (pattern: string, rule: RoutingRule) => {
      await routerApi.updateRoutingRule(pattern, rule);
      await refresh();
    };

    // Exclusion handlers
    const handleAddExclusion = async (
      provider: ProviderType,
      pattern: string,
    ) => {
      await routerApi.addExclusion(provider, pattern);
      await refresh();
    };

    const handleRemoveExclusion = async (
      provider: ProviderType,
      pattern: string,
    ) => {
      await routerApi.removeExclusion(provider, pattern);
      await refresh();
    };

    // Injection handlers
    const handleToggleInjection = async (enabled: boolean) => {
      await injectionApi.setInjectionEnabled(enabled);
      setInjectionEnabled(enabled);
    };

    const handleAddInjectionRule = async (rule: InjectionRule) => {
      await injectionApi.addInjectionRule(rule);
      await refresh();
    };

    const handleRemoveInjectionRule = async (id: string) => {
      await injectionApi.removeInjectionRule(id);
      await refresh();
    };

    const handleUpdateInjectionRule = async (
      id: string,
      rule: InjectionRule,
    ) => {
      await injectionApi.updateInjectionRule(id, rule);
      await refresh();
    };

    const tabs: { id: TabType; label: string; count: number }[] = [
      { id: "clients", label: "客户端路由", count: 0 },
      { id: "aliases", label: "模型别名", count: aliases.length },
      { id: "rules", label: "路由规则", count: rules.length },
      {
        id: "exclusions",
        label: "排除列表",
        count: Object.values(exclusions).flat().length,
      },
      { id: "injection", label: "参数注入", count: injectionRules.length },
    ];

    return (
      <div className="space-y-6">
        {!hideHeader && (
          <div className="flex items-center justify-between">
            <div>
              <h2 className="text-2xl font-bold flex items-center gap-2">
                <Route className="h-6 w-6" />
                智能路由
              </h2>
              <p className="text-muted-foreground">
                配置模型映射、路由规则和排除列表
              </p>
            </div>
            <div className="flex items-center gap-2">
              <button
                onClick={() => setShowPresets(true)}
                className="flex items-center gap-2 rounded-lg bg-primary px-3 py-2 text-sm text-primary-foreground hover:bg-primary/90"
              >
                <Sparkles className="h-4 w-4" />
                推荐配置
              </button>
              <button
                onClick={handleClearAll}
                disabled={
                  loading || (aliases.length === 0 && rules.length === 0)
                }
                className="flex items-center gap-2 rounded-lg border border-red-300 px-3 py-2 text-sm text-red-600 hover:bg-red-50 disabled:opacity-50 dark:border-red-800 dark:text-red-400 dark:hover:bg-red-950/30"
              >
                <Trash2 className="h-4 w-4" />
                清空
              </button>
              <button
                onClick={refresh}
                disabled={loading}
                className="flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
              >
                <RefreshCw
                  className={`h-4 w-4 ${loading ? "animate-spin" : ""}`}
                />
                刷新
              </button>
            </div>
          </div>
        )}

        {/* 当隐藏标题时，显示操作按钮 */}
        {hideHeader && (
          <div className="flex items-center justify-end gap-2">
            <button
              onClick={() => setShowPresets(true)}
              className="flex items-center gap-2 rounded-lg bg-primary px-3 py-2 text-sm text-primary-foreground hover:bg-primary/90"
            >
              <Sparkles className="h-4 w-4" />
              推荐配置
            </button>
            <button
              onClick={handleClearAll}
              disabled={loading || (aliases.length === 0 && rules.length === 0)}
              className="flex items-center gap-2 rounded-lg border border-red-300 px-3 py-2 text-sm text-red-600 hover:bg-red-50 disabled:opacity-50 dark:border-red-800 dark:text-red-400 dark:hover:bg-red-950/30"
            >
              <Trash2 className="h-4 w-4" />
              清空
            </button>
            <button
              onClick={refresh}
              disabled={loading}
              className="flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted disabled:opacity-50"
            >
              <RefreshCw
                className={`h-4 w-4 ${loading ? "animate-spin" : ""}`}
              />
              刷新
            </button>
          </div>
        )}

        {/* Presets Modal */}
        {showPresets && (
          <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
            <div className="w-full max-w-2xl rounded-lg bg-background p-6 shadow-xl max-h-[80vh] overflow-y-auto">
              <div className="flex items-center justify-between mb-4">
                <h3 className="text-lg font-semibold flex items-center gap-2">
                  <Sparkles className="h-5 w-5 text-primary" />
                  推荐配置
                </h3>
                <button
                  onClick={() => setShowPresets(false)}
                  className="text-muted-foreground hover:text-foreground"
                >
                  ✕
                </button>
              </div>
              <p className="text-sm text-muted-foreground mb-4">
                选择一个预设配置快速设置路由规则和模型别名
              </p>
              <div className="space-y-3">
                {presets.map((preset) => (
                  <div
                    key={preset.id}
                    className="rounded-lg border p-4 hover:border-primary/50 transition-colors"
                  >
                    <div className="flex items-start justify-between">
                      <div className="flex-1">
                        <h4 className="font-medium">{preset.name}</h4>
                        <p className="text-sm text-muted-foreground mt-1">
                          {preset.description}
                        </p>
                        <div className="flex gap-4 mt-2 text-xs text-muted-foreground">
                          <span>{preset.aliases.length} 个别名</span>
                          <span>{preset.rules.length} 条规则</span>
                          {preset.endpoint_providers && (
                            <span>
                              {
                                Object.values(preset.endpoint_providers).filter(
                                  (v) => v,
                                ).length
                              }{" "}
                              个客户端路由
                            </span>
                          )}
                        </div>
                      </div>
                      <div className="flex gap-2 ml-4">
                        <button
                          onClick={() => handleApplyPreset(preset.id, true)}
                          disabled={applyingPreset !== null}
                          className="flex items-center gap-1 rounded px-3 py-1.5 text-sm border hover:bg-muted disabled:opacity-50"
                        >
                          {applyingPreset === preset.id ? (
                            <RefreshCw className="h-3 w-3 animate-spin" />
                          ) : (
                            <Check className="h-3 w-3" />
                          )}
                          合并
                        </button>
                        <button
                          onClick={() => handleApplyPreset(preset.id, false)}
                          disabled={applyingPreset !== null}
                          className="flex items-center gap-1 rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                        >
                          {applyingPreset === preset.id ? (
                            <RefreshCw className="h-3 w-3 animate-spin" />
                          ) : (
                            <Check className="h-3 w-3" />
                          )}
                          应用
                        </button>
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </div>
        )}

        <HelpTip title="智能路由说明" variant="blue">
          <ul className="list-disc list-inside space-y-1 text-sm text-blue-700 dark:text-blue-400">
            <li>
              <span className="font-medium">模型别名</span>
              ：使用熟悉的模型名（如 gpt-4）映射到实际模型
            </li>
            <li>
              <span className="font-medium">路由规则</span>
              ：将特定模型路由到指定 Provider，支持通配符匹配
            </li>
            <li>
              <span className="font-medium">排除列表</span>
              ：从特定 Provider 排除某些模型
            </li>
            <li>精确匹配规则优先于通配符规则</li>
          </ul>
        </HelpTip>

        {error && (
          <div className="rounded-lg border border-red-500 bg-red-50 p-4 text-red-700 dark:bg-red-950/30">
            {error}
          </div>
        )}

        {/* Tabs */}
        <div className="flex gap-2 border-b">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`px-4 py-2 text-sm font-medium border-b-2 -mb-px flex items-center gap-2 ${
                activeTab === tab.id
                  ? "border-primary text-primary"
                  : "border-transparent text-muted-foreground hover:text-foreground"
              }`}
            >
              {tab.label}
              {tab.count > 0 && (
                <span className="rounded-full bg-muted px-1.5 py-0.5 text-xs">
                  {tab.count}
                </span>
              )}
            </button>
          ))}
        </div>

        {/* Tab content */}
        {loading ? (
          <div className="flex items-center justify-center py-12">
            <RefreshCw className="h-6 w-6 animate-spin text-muted-foreground" />
          </div>
        ) : (
          <div className="py-4">
            {activeTab === "aliases" && (
              <ModelMapping
                aliases={aliases}
                onAdd={handleAddAlias}
                onRemove={handleRemoveAlias}
                loading={loading}
              />
            )}
            {activeTab === "rules" && (
              <RoutingRules
                rules={rules}
                onAdd={handleAddRule}
                onRemove={handleRemoveRule}
                onUpdate={handleUpdateRule}
                loading={loading}
              />
            )}
            {activeTab === "clients" && <ClientRouting loading={loading} />}
            {activeTab === "exclusions" && (
              <ExclusionList
                exclusions={exclusions}
                onAdd={handleAddExclusion}
                onRemove={handleRemoveExclusion}
                loading={loading}
              />
            )}
            {activeTab === "injection" && (
              <InjectionRules
                rules={injectionRules}
                enabled={injectionEnabled}
                onToggleEnabled={handleToggleInjection}
                onAdd={handleAddInjectionRule}
                onRemove={handleRemoveInjectionRule}
                onUpdate={handleUpdateInjectionRule}
                loading={loading}
              />
            )}
          </div>
        )}
      </div>
    );
  },
);

RoutingPage.displayName = "RoutingPage";
