/**
 * API 中转上游供应商预设（OpenAI Chat Completions 兼容）。
 * 选择预设仅填充 Base URL / 名称，API Key 由用户填写。
 */
import type { ProviderPreset } from "./claudeProviderPresets";

export interface ApiRelayProviderPreset extends ProviderPreset {
  settingsConfig: {
    baseUrl: string;
    apiKey: string;
    name?: string;
  };
}

export const apiRelayProviderPresets: ApiRelayProviderPreset[] = [
  {
    name: "OpenRouter",
    websiteUrl: "https://openrouter.ai",
    apiKeyUrl: "https://openrouter.ai/keys",
    category: "aggregator",
    settingsConfig: {
      baseUrl: "https://openrouter.ai/api/v1",
      apiKey: "",
    },
  },
  {
    name: "DeepSeek",
    websiteUrl: "https://platform.deepseek.com",
    apiKeyUrl: "https://platform.deepseek.com/api_keys",
    category: "cn_official",
    settingsConfig: {
      baseUrl: "https://api.deepseek.com/v1",
      apiKey: "",
    },
  },
  {
    name: "Moonshot Kimi",
    websiteUrl: "https://platform.moonshot.cn",
    apiKeyUrl: "https://platform.moonshot.cn/console/api-keys",
    category: "cn_official",
    settingsConfig: {
      baseUrl: "https://api.moonshot.cn/v1",
      apiKey: "",
    },
  },
  {
    name: "智谱 GLM",
    websiteUrl: "https://open.bigmodel.cn",
    apiKeyUrl: "https://open.bigmodel.cn/usercenter/apikeys",
    category: "cn_official",
    settingsConfig: {
      baseUrl: "https://open.bigmodel.cn/api/paas/v4",
      apiKey: "",
    },
  },
  {
    name: "通义千问",
    websiteUrl: "https://dashscope.aliyun.com",
    apiKeyUrl: "https://dashscope.console.aliyun.com/apiKey",
    category: "cn_official",
    settingsConfig: {
      baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
      apiKey: "",
    },
  },
  {
    name: "硅基流动 SiliconFlow",
    websiteUrl: "https://siliconflow.cn",
    apiKeyUrl: "https://cloud.siliconflow.cn/account/ak",
    category: "third_party",
    settingsConfig: {
      baseUrl: "https://api.siliconflow.cn/v1",
      apiKey: "",
    },
  },
  {
    name: "火山方舟",
    websiteUrl: "https://www.volcengine.com/product/ark",
    apiKeyUrl: "https://console.volcengine.com/ark",
    category: "cn_official",
    settingsConfig: {
      baseUrl: "https://ark.cn-beijing.volces.com/api/v3",
      apiKey: "",
    },
  },
];
