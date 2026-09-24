// [desensitize] 隐私保护中心 API 封装
import { invoke } from "@tauri-apps/api/core";

export interface DesensitizeDashboard {
  today_hits: number;
  total_hits: number;
  restore_rate: number;
  mapping_count: number;
  active_sessions: number;
  type_distribution: { entity_type: string; count: number }[];
  recent_hits: DesensitizeHitLog[];
}

export interface DesensitizeHitLog {
  session_key: string;
  direction: "in" | "out";
  original_masked: string;
  placeholder: string;
  entity_type: string;
  hit_source: string;
  confidence: number;
  restored: boolean;
  context: string;
  placeholder_context: string;
  created_at: number;
}

export interface DesensitizeRule {
  name: string;
  entity_type: string;
  category: string;
  pattern: string;
  enabled: boolean;
}

export interface DesensitizeRuleCategory {
  category: string;
  label: string;
  count: number;
  rules: DesensitizeRule[];
}

export interface DesensitizeNerCategory {
  entity_type: string;
  name: string;
  desc: string;
  enabled: boolean;
}

export interface DesensitizeModelStatus {
  embedding: {
    downloaded: boolean;
    path: string | null;
    size_mb: number;
    ready: boolean;
    error: string | null;
  };
}

export interface DesensitizeKeyword {
  id: number;
  keyword: string;
  entity_type: string;
  match_mode: string;
  threshold: number;
  enabled: boolean;
}

export interface DesensitizeMapping {
  id: number;
  session_key: string;
  placeholder: string;
  original: string;
  entity_type: string;
  hit_source: string;
  confidence: number;
  created_at: number;
}

export interface DesensitizePreview {
  hits: {
    entity_type: string;
    source: string;
    confidence: number;
    original: string;
    original_masked: string;
    placeholder: string;
    context: string;
    placeholder_context: string;
  }[];
  masked: string;
  placeholderized: string;
}

export const desensitizeApi = {
  dashboard: () =>
    invoke<DesensitizeDashboard>("get_desensitize_dashboard"),

  listRules: () => invoke<DesensitizeRule[]>("list_desensitize_rules"),

  listRuleCategories: () =>
    invoke<DesensitizeRuleCategory[]>("list_desensitize_rule_categories"),

  setRuleEnabled: (entityType: string, enabled: boolean) =>
    invoke<void>("set_desensitize_rule_enabled", { entityType, enabled }),

  listKeywords: () =>
    invoke<DesensitizeKeyword[]>("list_desensitize_keywords"),

  getKeywordVariants: (keyword: string) =>
    invoke<{ word: string; source: string }[]>(
      "get_desensitize_keyword_variants",
      { keyword },
    ),

  addKeyword: (payload: {
    keyword: string;
    entityType?: string;
    matchMode?: string;
    threshold?: number;
  }) =>
    invoke<number>("add_desensitize_keyword", {
      keyword: payload.keyword,
      entityType: payload.entityType,
      matchMode: payload.matchMode,
      threshold: payload.threshold,
    }),

  updateKeywordMode: (id: number, matchMode: string) =>
    invoke<void>("update_desensitize_keyword_mode", { id, matchMode }),

  removeKeyword: (id: number) =>
    invoke<void>("remove_desensitize_keyword", { id }),

  exportKeywords: (path: string) =>
    invoke<{ count: number; path: string }>("export_desensitize_keywords", {
      path,
    }),

  importKeywords: (path: string) =>
    invoke<{ imported: number; skipped: number }>(
      "import_desensitize_keywords",
      { path },
    ),

  queryLogs: (limit?: number, offset?: number) =>
    invoke<{ logs: DesensitizeHitLog[] }>("query_desensitize_logs", {
      limit,
      offset,
    }),

  preview: (text: string, threshold?: number) =>
    invoke<DesensitizePreview>("preview_desensitize", {
      text,
      threshold,
    }),

  getModelStatus: () =>
    invoke<DesensitizeModelStatus>("get_desensitize_model_status"),

  setPiiEnabled: (enabled: boolean) =>
    invoke<void>("set_desensitize_pii_enabled", { enabled }),

  setKeywordEnabled: (enabled: boolean) =>
    invoke<void>("set_desensitize_keyword_enabled", { enabled }),

  getPiiEnabled: () =>
    invoke<{ enabled: boolean }>("get_desensitize_pii_enabled"),

  getSemanticThreshold: () =>
    invoke<{ threshold: number }>("get_desensitize_semantic_threshold"),

  setSemanticThreshold: (threshold: number) =>
    invoke<void>("set_desensitize_semantic_threshold", { threshold }),

  getKeywordEnabled: () =>
    invoke<{ enabled: boolean }>("get_desensitize_keyword_enabled"),

  clearMappings: (sessionKey?: string) =>
    invoke<{ deleted: number }>("clear_desensitize_mappings", {
      sessionKey,
    }),

  getEnabled: () =>
    invoke<{ enabled: boolean }>("get_desensitize_enabled"),

  setEnabled: (enabled: boolean) =>
    invoke<void>("set_desensitize_enabled", { enabled }),

  getSkipSemanticLarge: () =>
    invoke<{ skipSemanticLarge: boolean }>(
      "get_desensitize_skip_semantic_large",
    ),

  setSkipSemanticLarge: (skip: boolean) =>
    invoke<void>("set_desensitize_skip_semantic_large", { skip }),

  getScope: () =>
    invoke<{ scope: "user" | "all" }>("get_desensitize_scope"),

  setScope: (scope: "user" | "all") =>
    invoke<void>("set_desensitize_scope", { scope }),

  listMappings: (params?: {
    search?: string;
    sessionKey?: string;
    limit?: number;
    offset?: number;
  }) =>
    invoke<{ total: number; items: DesensitizeMapping[] }>(
      "list_desensitize_mappings",
      {
        search: params?.search,
        sessionKey: params?.sessionKey,
        limit: params?.limit,
        offset: params?.offset,
      },
    ),

  deleteMapping: (id: number) =>
    invoke<void>("delete_desensitize_mapping", { id }),

  getPlaceholderPrefix: () =>
    invoke<{ prefix: string }>("get_desensitize_placeholder_prefix"),

  setPlaceholderPrefix: (prefix: string) =>
    invoke<void>("set_desensitize_placeholder_prefix", { prefix }),
};
