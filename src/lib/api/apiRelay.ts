import { invoke } from "@tauri-apps/api/core";

export interface ApiRelayUpstream {
  id: string;
  name: string;
  base_url: string;
  api_key_set: boolean;
}

export interface ApiRelayInfo {
  running: boolean;
  port: number;
  address: string;
  key: string;
  upstream: ApiRelayUpstream | null;
}

export interface ApiRelayLog {
  id: number;
  created_at: number;
  model: string;
  provider: string;
  request_summary: string;
  response_summary: string;
  status: number;
  latency_ms: number;
}

export const apiRelayApi = {
  getInfo: () => invoke<ApiRelayInfo>("get_api_relay_info"),
  regenerateKey: () => invoke<{ key: string }>("regenerate_api_relay_key"),
  listLogs: (limit = 50, offset = 0) =>
    invoke<{ logs: ApiRelayLog[] }>("list_api_relay_logs", { limit, offset }),
  clearLogs: () => invoke<void>("clear_api_relay_logs"),
};
