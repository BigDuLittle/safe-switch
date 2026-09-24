// [api-relay] API 中转面板：一行展示本地 OpenAI 兼容入口（Base URL + 本地 Key + 重新生成），
// 带开关可手动启停中转路由，"?" 弹出使用说明。上游供应商的添加 / 编辑 / 切换由下方复用的 ProviderList 管理（app_type=api-relay）。
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Copy, Check, RefreshCw, HelpCircle, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { invoke } from "@tauri-apps/api/core";
import { apiRelayApi, type ApiRelayInfo } from "@/lib/api/apiRelay";

function CopyChip({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <div className="flex items-center gap-1.5 min-w-0">
      <span className="text-xs text-muted-foreground shrink-0">{label}</span>
      <button
        type="button"
        onClick={() => void copy()}
        title={t("apiRelay.copy")}
        className={`inline-flex items-center gap-1.5 rounded-lg border bg-muted/40 px-2.5 py-1.5 text-xs hover:bg-muted/70 transition-colors min-w-0 ${
          mono ? "font-mono" : ""
        }`}
      >
        <span className="truncate max-w-[280px]">{value}</span>
        {copied ? (
          <Check className="w-3 h-3 text-green-600 shrink-0" />
        ) : (
          <Copy className="w-3 h-3 shrink-0 text-muted-foreground" />
        )}
      </button>
    </div>
  );
}

export function ApiRelayPanel() {
  const { t } = useTranslation();
  const [info, setInfo] = useState<ApiRelayInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [regenerating, setRegenerating] = useState(false);
  const [toggling, setToggling] = useState(false);

  const load = useCallback(async () => {
    try {
      const data = await apiRelayApi.getInfo();
      setInfo(data);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const regenerate = async () => {
    setRegenerating(true);
    try {
      const { key } = await apiRelayApi.regenerateKey();
      setInfo((prev) => (prev ? { ...prev, key } : prev));
      toast.success(t("apiRelay.keyRegenerated"));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setRegenerating(false);
    }
  };

  const toggle = async () => {
    if (toggling) return;
    setToggling(true);
    try {
      if (info?.running) {
        await invoke("stop_proxy_server");
        toast.success("中转路由已关闭");
      } else {
        await invoke("start_proxy_server");
        toast.success("中转路由已开启");
      }
      await load();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setToggling(false);
    }
  };

  const baseUrl = info ? `http://127.0.0.1:${info.port}/vault/v1` : "";

  if (loading) {
    return (
      <div className="py-4 flex items-center justify-center text-muted-foreground">
        <Loader2 className="w-4 h-4 mr-2 animate-spin" />
        {t("apiRelay.loading")}
      </div>
    );
  }

  const running = !!info?.running;

  return (
    <div className="rounded-xl border bg-card px-4 py-3 flex items-center gap-3 flex-wrap">
      <div className="flex items-center gap-2 shrink-0">
        <button
          type="button"
          onClick={() => void toggle()}
          disabled={toggling}
          className={`relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50 ${
            running ? "bg-green-600" : "bg-muted-foreground/30"
          }`}
        >
          <span
            className={`pointer-events-none block h-4 w-4 rounded-full bg-white shadow-lg ring-0 transition-transform ${
              running ? "translate-x-4" : "translate-x-0.5"
            }`}
          />
        </button>
        <span className="text-xs text-muted-foreground">
          {running ? "已开启" : "已关闭"}
        </span>
      </div>
      <CopyChip label={t("apiRelay.baseUrl")} value={baseUrl} mono />
      <CopyChip
        label={t("apiRelay.localKey")}
        value={info?.key ?? ""}
        mono
      />
      <div className="flex items-center gap-1 shrink-0">
        <TooltipProvider delayDuration={250}>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                onClick={() => void regenerate()}
                disabled={regenerating}
                aria-label={t("apiRelay.regenerateKey")}
              >
                <RefreshCw
                  className={`w-3.5 h-3.5 ${regenerating ? "animate-spin" : ""}`}
                />
              </Button>
            </TooltipTrigger>
            <TooltipContent>{t("apiRelay.regenerateKey")}</TooltipContent>
          </Tooltip>
        </TooltipProvider>
        <Popover>
        <PopoverTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 shrink-0"
            aria-label={t("apiRelay.howToTitle")}
          >
            <HelpCircle className="w-4 h-4 text-muted-foreground" />
          </Button>
        </PopoverTrigger>
        <PopoverContent className="w-80 text-xs space-y-1.5" align="end">
          <p className="text-sm font-medium text-foreground">
            {t("apiRelay.howToTitle")}
          </p>
          <p className="text-muted-foreground">{t("apiRelay.howTo1")}</p>
          <p className="text-muted-foreground">{t("apiRelay.howTo2")}</p>
          <p className="text-muted-foreground">{t("apiRelay.howTo3")}</p>
        </PopoverContent>
        </Popover>
      </div>
    </div>
  );
}
