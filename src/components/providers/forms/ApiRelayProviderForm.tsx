import { useCallback, useEffect, useState } from "react";
import { useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Form,
  FormField,
  FormItem,
  FormMessage,
} from "@/components/ui/form";
import { Label } from "@/components/ui/label";
import { ImeSafeInput } from "@/components/ui/ime-safe-input";
import type { ProviderFormData } from "@/lib/schemas/provider";
import type { ProviderCategory } from "@/types";
import { BasicFormFields } from "./BasicFormFields";
import { ProviderPresetSelector } from "./ProviderPresetSelector";
import { HermesFormFields } from "./HermesFormFields";
import {
  HERMES_DEFAULT_CONFIG,
  useHermesFormState,
} from "./hooks/useHermesFormState";
import { hermesProviderPresets } from "@/config/hermesProviderPresets";
import type {
  ProviderFormProps,
  ProviderFormValues,
} from "./ProviderForm";

const PRESET_ENTRIES = hermesProviderPresets.map((preset, index) => ({
  id: String(index),
  preset,
}));

const normalizeProviderKey = (value: string) =>
  value.toLowerCase().replace(/[^a-z0-9-]/g, "");

/** 兼容旧格式 {baseUrl, apiKey} → Hermes 格式 {base_url, api_key} */
function toHermesConfig(raw: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = { ...raw };
  if (out.baseUrl && !out.base_url) out.base_url = out.baseUrl;
  if (out.apiKey && !out.api_key) out.api_key = out.apiKey;
  return out;
}

/**
 * API 中转上游供应商表单：复用 Hermes 供应商表单形态
 * （供应商标识 + 基础字段 + API 模式/API 端点/API Key/模型列表/请求间隔），
 * 不展示配置工具 JSON。settings_config 与 Hermes 同构（base_url/api_key/...），
 * 后端读取时兼容 baseUrl/apiKey 两种格式。
 */
export function ApiRelayProviderForm({
  initialData,
  providerId,
  onSubmit,
  onCancel,
  submitLabel,
  showButtons = true,
  onSubmittingChange,
  onSubmitReadyChange,
}: ProviderFormProps) {
  const { t } = useTranslation();
  const isEditMode = Boolean(initialData);
  const [providerKey, setProviderKey] = useState(providerId ?? "");
  const [selectedPresetId, setSelectedPresetId] = useState("custom");
  const [category, setCategory] = useState<ProviderCategory>(
    initialData?.category ?? "custom",
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const form = useForm<ProviderFormData>({
    defaultValues: {
      name: initialData?.name ?? "",
      notes: initialData?.notes ?? "",
      websiteUrl: initialData?.websiteUrl ?? "",
      icon: initialData?.icon ?? "",
      iconColor: initialData?.iconColor ?? "",
      settingsConfig: JSON.stringify(
        toHermesConfig(initialData?.settingsConfig ?? {}),
        null,
        2,
      ),
    },
  });

  const getSettingsConfig = useCallback(
    () => form.getValues("settingsConfig") || HERMES_DEFAULT_CONFIG,
    [form],
  );
  const onSettingsConfigChange = useCallback(
    (config: string) => form.setValue("settingsConfig", config),
    [form],
  );

  const hermesForm = useHermesFormState({
    initialData: {
      settingsConfig: toHermesConfig(initialData?.settingsConfig ?? {}),
    },
    appId: "hermes",
    providerId,
    onSettingsConfigChange,
    getSettingsConfig,
  });

  const name = form.watch("name");
  const ready = Boolean(
    name.trim() &&
      providerKey.trim() &&
      hermesForm.hermesBaseUrl.trim() &&
      hermesForm.hermesApiKey.trim(),
  );
  useEffect(() => {
    onSubmitReadyChange?.(ready);
  }, [ready, onSubmitReadyChange]);

  const choosePreset = (id: string) => {
    setSelectedPresetId(id);
    if (id === "custom") return;
    const preset = hermesProviderPresets[Number(id)];
    if (!preset) return;
    hermesForm.resetHermesState(preset.settingsConfig);
    setCategory(preset.category ?? "custom");
    form.reset({
      name: preset.name,
      websiteUrl: preset.websiteUrl ?? "",
      notes: "",
      icon: preset.icon ?? "",
      iconColor: preset.iconColor ?? "",
      settingsConfig: JSON.stringify(preset.settingsConfig, null, 2),
    });
  };

  const submit = async (identity: ProviderFormData) => {
    if (busy) return;
    setBusy(true);
    onSubmittingChange?.(true);
    setError("");
    try {
      const key = providerKey.trim();
      if (!key) {
        setError(
          t("hermes.form.providerKeyRequired", {
            defaultValue: "供应商标识不能为空",
          }),
        );
        return;
      }
      if (!/^[a-z0-9]+(-[a-z0-9]+)*$/.test(key)) {
        setError(
          t("hermes.form.providerKeyInvalid", {
            defaultValue: "供应商标识只能使用小写字母、数字和连字符",
          }),
        );
        return;
      }
      const selectedPreset =
        selectedPresetId === "custom"
          ? undefined
          : hermesProviderPresets[Number(selectedPresetId)];
      const values: ProviderFormValues = {
        ...identity,
        name: identity.name.trim(),
        providerKey: key,
        presetId: selectedPreset ? selectedPresetId : undefined,
        presetCategory: selectedPreset?.category ?? category,
        meta: initialData?.meta,
        settingsConfig: getSettingsConfig(),
      };
      await onSubmit(values);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      onSubmittingChange?.(false);
    }
  };

  return (
    <Form {...form}>
      <form
        id="provider-form"
        className="space-y-6 glass rounded-xl p-6 border border-white/10"
        onSubmit={form.handleSubmit((identity) => submit(identity))}
      >
        {!isEditMode && (
          <ProviderPresetSelector
            selectedPresetId={selectedPresetId}
            presetEntries={PRESET_ENTRIES}
            presetCategoryLabels={{
              custom: t("providerPreset.custom"),
              cn_official: t("providerForm.categoryCnOfficial"),
              aggregator: t("providerForm.categoryAggregation"),
              third_party: t("providerForm.categoryThirdParty"),
              official: t("providerForm.categoryOfficial"),
            }}
            onPresetChange={choosePreset}
            category={category}
          />
        )}
        {error && (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        )}

        {/* 供应商标识 */}
        <div className="space-y-2">
          <Label htmlFor="apirelay-key">
            {t("hermes.form.providerKey", {
              defaultValue: "供应商标识",
            })}
            <span className="text-destructive ml-1">*</span>
          </Label>
          <ImeSafeInput
            id="apirelay-key"
            value={providerKey}
            onValueChange={setProviderKey}
            normalize={normalizeProviderKey}
            placeholder={t("hermes.form.providerKeyPlaceholder", {
              defaultValue: "my-provider",
            })}
            disabled={isEditMode}
            className={
              providerKey.trim() !== "" &&
              !/^[a-z0-9]+(-[a-z0-9]+)*$/.test(providerKey)
                ? "border-destructive"
                : ""
            }
          />
          {providerKey.trim() !== "" &&
            !/^[a-z0-9]+(-[a-z0-9]+)*$/.test(providerKey) && (
              <p className="text-xs text-destructive">
                {t("hermes.form.providerKeyInvalid", {
                  defaultValue: "供应商标识只能使用小写字母、数字和连字符",
                })}
              </p>
            )}
          <p className="text-xs text-muted-foreground">
            {isEditMode
              ? t("hermes.form.providerKeyLockedHint", {
                  defaultValue: "已添加的供应商标识不可修改",
                })
              : t("hermes.form.providerKeyHint", {
                  defaultValue:
                    "只能使用小写字母、数字和连字符。作为该上游供应商的唯一标识。",
                })}
          </p>
        </div>

        <BasicFormFields form={form} />

        <HermesFormFields
          baseUrl={hermesForm.hermesBaseUrl}
          onBaseUrlChange={hermesForm.handleHermesBaseUrlChange}
          apiKey={hermesForm.hermesApiKey}
          onApiKeyChange={hermesForm.handleHermesApiKeyChange}
          category={category}
          shouldShowApiKeyLink={true}
          websiteUrl={form.watch("websiteUrl") ?? ""}
          apiMode={hermesForm.hermesApiMode}
          onApiModeChange={hermesForm.handleHermesApiModeChange}
          models={hermesForm.hermesModels}
          onModelsChange={hermesForm.handleHermesModelsChange}
          rateLimitDelay={hermesForm.hermesRateLimitDelay}
          onRateLimitDelayChange={hermesForm.handleHermesRateLimitDelayChange}
        />

        <FormField
          control={form.control}
          name="settingsConfig"
          render={() => (
            <FormItem className="space-y-0">
              <FormMessage />
            </FormItem>
          )}
        />

        {showButtons && (
          <div className="flex justify-end gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={onCancel}
              disabled={busy}
            >
              {t("common.cancel")}
            </Button>
            <Button type="submit" disabled={busy || !ready}>
              {submitLabel}
            </Button>
          </div>
        )}
      </form>
    </Form>
  );
}
