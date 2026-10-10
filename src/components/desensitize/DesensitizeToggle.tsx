// [desensitize] 数据安全防护快捷开关（顶栏，样式与路由总开关一致）
// 点击左侧图标+文字进入隐私保护配置页，右侧开关快速启停
import { useEffect, useState } from "react";
import { ShieldCheck, Loader2 } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";
import { useTranslation } from "react-i18next";
import { desensitizeApi } from "@/lib/api/desensitize";

interface DesensitizeToggleProps {
  className?: string;
  onOpenSettings?: () => void;
}

export function DesensitizeToggle({
  className,
  onOpenSettings,
}: DesensitizeToggleProps) {
  const { t } = useTranslation();
  const [enabled, setEnabled] = useState(true);
  const [loaded, setLoaded] = useState(false);
  const [pending, setPending] = useState(false);

  useEffect(() => {
    desensitizeApi
      .getEnabled()
      .then((r) => {
        setEnabled(r.enabled);
        setLoaded(true);
      })
      .catch(() => setLoaded(true));
  }, []);

  const handleToggle = async (checked: boolean) => {
    const prev = enabled;
    setPending(true);
    setEnabled(checked); // 乐观更新
    try {
      await desensitizeApi.setEnabled(checked);
    } catch (error) {
      console.error("[DesensitizeToggle] toggle failed:", error);
      setEnabled(prev); // 失败回滚
    } finally {
      setPending(false);
    }
  };

  const label = t("desensitize.toggleLabel", {
    defaultValue: "隐私保护",
  });

  return (
    <div
      className={cn(
        "flex items-center gap-1.5 px-2 h-8 rounded-lg bg-muted/50 transition-all",
        className,
      )}
      style={{ WebkitAppRegion: "no-drag" } as any}
    >
      {/* 图标 + 文字：点击进入设置 */}
      <button
        type="button"
        onClick={onOpenSettings}
        title={t("desensitize.openSettings", { defaultValue: "隐私保护设置" })}
        className="flex items-center gap-1.5 -ml-0.5 px-1 py-0.5 rounded-md hover:bg-accent/60 transition-colors"
        style={{ WebkitAppRegion: "no-drag" } as any}
      >
        {pending ? (
          <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
        ) : (
          <span
            className={cn(
              "inline-flex items-center justify-center w-5 h-5 rounded-full border transition-colors",
              enabled
                ? "border-emerald-500/40 bg-emerald-500/10"
                : "border-muted-foreground/30 bg-muted-foreground/5",
            )}
          >
            <ShieldCheck
              className={cn(
                "h-3 w-3",
                enabled ? "text-emerald-500" : "text-muted-foreground",
              )}
            />
          </span>
        )}
        <span
          className={cn(
            "text-xs whitespace-nowrap",
            enabled ? "text-foreground" : "text-muted-foreground",
          )}
        >
          {label}
        </span>
      </button>
      <Switch
        checked={enabled}
        onCheckedChange={handleToggle}
        disabled={pending || !loaded}
        aria-label={label}
      />
    </div>
  );
}
