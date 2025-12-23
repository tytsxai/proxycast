import { invoke } from "@tauri-apps/api/core";

// Provider types
export type ProviderType =
  | "kiro"
  | "gemini"
  | "qwen"
  | "antigravity"
  | "openai"
  | "claude";

// Model alias mapping
export interface ModelAlias {
  alias: string;
  actual: string;
}

// Routing rule
export interface RoutingRule {
  pattern: string;
  target_provider: ProviderType;
  priority: number;
  enabled: boolean;
}

// Exclusion pattern
export interface ExclusionPattern {
  provider: ProviderType;
  pattern: string;
}

// Recommended preset
export interface RecommendedPreset {
  id: string;
  name: string;
  description: string;
  aliases: ModelAlias[];
  rules: RoutingRule[];
  endpoint_providers?: {
    cursor?: string;
    claude_code?: string;
    codex?: string;
    windsurf?: string;
    kiro?: string;
    other?: string;
  };
}

// Router configuration
export interface RouterConfig {
  default_provider: ProviderType;
  aliases: ModelAlias[];
  rules: RoutingRule[];
  exclusions: Record<ProviderType, string[]>;
}

export const routerApi = {
  // Get router configuration
  async getRouterConfig(): Promise<RouterConfig> {
    return invoke("get_router_config");
  },

  // Model aliases
  async addModelAlias(alias: string, actual: string): Promise<void> {
    return invoke("add_model_alias", { alias, actual });
  },

  async removeModelAlias(alias: string): Promise<void> {
    return invoke("remove_model_alias", { alias });
  },

  async getModelAliases(): Promise<ModelAlias[]> {
    return invoke("get_model_aliases");
  },

  // Routing rules
  async addRoutingRule(rule: RoutingRule): Promise<void> {
    return invoke("add_routing_rule", { rule });
  },

  async removeRoutingRule(pattern: string): Promise<void> {
    return invoke("remove_routing_rule", { pattern });
  },

  async updateRoutingRule(pattern: string, rule: RoutingRule): Promise<void> {
    return invoke("update_routing_rule", { pattern, rule });
  },

  async getRoutingRules(): Promise<RoutingRule[]> {
    return invoke("get_routing_rules");
  },

  // Exclusions
  async addExclusion(provider: ProviderType, pattern: string): Promise<void> {
    return invoke("add_exclusion", { provider, pattern });
  },

  async removeExclusion(
    provider: ProviderType,
    pattern: string,
  ): Promise<void> {
    return invoke("remove_exclusion", { provider, pattern });
  },

  async getExclusions(): Promise<Record<ProviderType, string[]>> {
    return invoke("get_exclusions");
  },

  // Default provider
  async setDefaultProvider(provider: ProviderType): Promise<void> {
    return invoke("set_router_default_provider", { provider });
  },

  // Recommended presets
  async getRecommendedPresets(): Promise<RecommendedPreset[]> {
    return invoke("get_recommended_presets");
  },

  async applyRecommendedPreset(
    presetId: string,
    merge: boolean = false,
  ): Promise<void> {
    return invoke("apply_recommended_preset", { presetId, merge });
  },

  async clearAllRoutingConfig(): Promise<void> {
    return invoke("clear_all_routing_config");
  },
};
