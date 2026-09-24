// [desensitize] 隐私保护中心面板（两策略：内置 PII / 自定义关键词）
import React, { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ShieldCheck,
  Activity,
  CheckCircle2,
  Database,
  Users,
  Plus,
  X,
  Play,
  Trash2,
  Loader2,
  EyeOff,
  ChevronDown,
  Download,
  Upload,
  RefreshCw,
  Search,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Badge } from "@/components/ui/badge";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

// [desensitize] 把上下文片段里的命中词高亮显示
function HighlightHit({ text, hit }: { text: string; hit: string }) {
  if (!hit) return <span className="text-xs">{text}</span>;
  const idx = text.indexOf(hit);
  if (idx < 0) return <span className="text-xs">{text}</span>;
  return (
    <span className="text-xs break-all">
      {text.slice(0, idx)}
      <mark className="bg-orange-200 text-orange-900 rounded px-0.5">
        {hit}
      </mark>
      {text.slice(idx + hit.length)}
    </span>
  );
}
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@/components/ui/tabs";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  desensitizeApi,
  type DesensitizeDashboard,
  type DesensitizeHitLog,
  type DesensitizeKeyword,
  type DesensitizeMapping,
  type DesensitizeModelStatus,
  type DesensitizePreview,
  type DesensitizeRule,
  type DesensitizeRuleCategory,
} from "@/lib/api/desensitize";
import { cn } from "@/lib/utils";

const typeLabel: Record<string, string> = {
  // 策略① 身份信息
  cn_id_card: "身份证号",
  cn_driver_license: "驾驶证号",
  cn_passport: "护照号",
  cn_entry_permit: "通行证",
  intl_ssn: "国际社保号",
  // 策略① 联系方式
  cn_phone: "手机号",
  intl_phone: "国际电话",
  cn_tel: "固定电话",
  email: "邮箱",
  // 策略① 财务
  bank_card: "银行卡",
  credit_card: "信用卡",
  iban: "IBAN",
  // 策略① 网络
  intranet_ip: "内网 IP",
  public_ip: "公网 IP",
  ipv6: "IPv6",
  intranet_domain: "内网域名",
  mac: "MAC 地址",
  db_uri: "数据库连接串",
  // 策略① 凭据
  openai_key: "OpenAI Key",
  anthropic_key: "Anthropic Key",
  google_key: "Google Key",
  github_token: "GitHub Token",
  aws_key: "AWS Key",
  jwt: "JWT",
  private_key: "私钥",
  other_token: "其他 Token",
  // 策略① 位置
  cn_plate: "车牌号",
  // 策略③ NER 实体（14 类）
  name: "人名",
  company: "公司",
  organization: "组织",
  position: "职位",
  address: "地址",
  government: "政府机构",
  mobile: "手机号",
  QQ: "QQ号",
  vx: "微信号",
  book: "书名",
  movie: "电影",
  game: "游戏",
  scene: "景点",
  // 策略② 用户关键词
  keyword: "关键词",
};

const typeColor: Record<string, string> = {
  cn_id_card: "bg-red-100 text-red-700 dark:bg-red-500/15 dark:text-red-400",
  cn_driver_license:
    "bg-red-100 text-red-700 dark:bg-red-500/15 dark:text-red-400",
  cn_passport: "bg-red-100 text-red-700 dark:bg-red-500/15 dark:text-red-400",
  cn_entry_permit:
    "bg-red-100 text-red-700 dark:bg-red-500/15 dark:text-red-400",
  intl_ssn: "bg-red-100 text-red-700 dark:bg-red-500/15 dark:text-red-400",
  cn_phone: "bg-orange-100 text-orange-700 dark:bg-orange-500/15 dark:text-orange-400",
  intl_phone: "bg-orange-100 text-orange-700 dark:bg-orange-500/15 dark:text-orange-400",
  cn_tel: "bg-orange-100 text-orange-700 dark:bg-orange-500/15 dark:text-orange-400",
  email: "bg-green-100 text-green-700 dark:bg-green-500/15 dark:text-green-400",
  bank_card: "bg-amber-100 text-amber-700 dark:bg-amber-500/15 dark:text-amber-400",
  credit_card: "bg-amber-100 text-amber-700 dark:bg-amber-500/15 dark:text-amber-400",
  iban: "bg-amber-100 text-amber-700 dark:bg-amber-500/15 dark:text-amber-400",
  intranet_ip: "bg-blue-100 text-blue-700 dark:bg-blue-500/15 dark:text-blue-400",
  public_ip: "bg-blue-100 text-blue-700 dark:bg-blue-500/15 dark:text-blue-400",
  ipv6: "bg-blue-100 text-blue-700 dark:bg-blue-500/15 dark:text-blue-400",
  intranet_domain:
    "bg-violet-100 text-violet-700 dark:bg-violet-500/15 dark:text-violet-400",
  mac: "bg-blue-100 text-blue-700 dark:bg-blue-500/15 dark:text-blue-400",
  db_uri: "bg-blue-100 text-blue-700 dark:bg-blue-500/15 dark:text-blue-400",
  openai_key: "bg-purple-100 text-purple-700 dark:bg-purple-500/15 dark:text-purple-400",
  anthropic_key: "bg-purple-100 text-purple-700 dark:bg-purple-500/15 dark:text-purple-400",
  google_key: "bg-purple-100 text-purple-700 dark:bg-purple-500/15 dark:text-purple-400",
  github_token: "bg-purple-100 text-purple-700 dark:bg-purple-500/15 dark:text-purple-400",
  aws_key: "bg-purple-100 text-purple-700 dark:bg-purple-500/15 dark:text-purple-400",
  jwt: "bg-purple-100 text-purple-700 dark:bg-purple-500/15 dark:text-purple-400",
  private_key: "bg-purple-100 text-purple-700 dark:bg-purple-500/15 dark:text-purple-400",
  other_token: "bg-purple-100 text-purple-700 dark:bg-purple-500/15 dark:text-purple-400",
  cn_plate: "bg-teal-100 text-teal-700 dark:bg-teal-500/15 dark:text-teal-400",
  name: "bg-cyan-100 text-cyan-700 dark:bg-cyan-500/15 dark:text-cyan-400",
  company: "bg-cyan-100 text-cyan-700 dark:bg-cyan-500/15 dark:text-cyan-400",
  organization: "bg-cyan-100 text-cyan-700 dark:bg-cyan-500/15 dark:text-cyan-400",
  position: "bg-cyan-100 text-cyan-700 dark:bg-cyan-500/15 dark:text-cyan-400",
  address: "bg-cyan-100 text-cyan-700 dark:bg-cyan-500/15 dark:text-cyan-400",
  government: "bg-cyan-100 text-cyan-700 dark:bg-cyan-500/15 dark:text-cyan-400",
  mobile: "bg-cyan-100 text-cyan-700 dark:bg-cyan-500/15 dark:text-cyan-400",
  QQ: "bg-cyan-100 text-cyan-700 dark:bg-cyan-500/15 dark:text-cyan-400",
  vx: "bg-cyan-100 text-cyan-700 dark:bg-cyan-500/15 dark:text-cyan-400",
  book: "bg-cyan-100 text-cyan-700 dark:bg-cyan-500/15 dark:text-cyan-400",
  movie: "bg-cyan-100 text-cyan-700 dark:bg-cyan-500/15 dark:text-cyan-400",
  game: "bg-cyan-100 text-cyan-700 dark:bg-cyan-500/15 dark:text-cyan-400",
  scene: "bg-cyan-100 text-cyan-700 dark:bg-cyan-500/15 dark:text-cyan-400",
  keyword: "bg-violet-100 text-violet-700 dark:bg-violet-500/15 dark:text-violet-400",
};

const sourceLabel: Record<string, string> = {
  regex: "正则规则",
  keyword: "关键词",
  semantic: "嵌入/语义",
  ner: "NER 模型",
  restore: "还原",
};

function fmtTime(ts: number): string {
  if (!ts) return "-";
  const d = new Date(ts * 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

function fmtDateTime(ts: number): string {
  if (!ts) return "-";
  const d = new Date(ts * 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

function StatCard({
  icon,
  iconClass,
  label,
  value,
  sub,
}: {
  icon: React.ReactNode;
  iconClass: string;
  label: string;
  value: React.ReactNode;
  sub?: React.ReactNode;
}) {
  return (
    <div className="flex-1 min-w-[150px] rounded-xl border bg-card p-4 flex flex-col gap-1">
      <div className="flex items-center gap-2 text-muted-foreground">
        <span
          className={cn(
            "flex h-7 w-7 items-center justify-center rounded-lg",
            iconClass,
          )}
        >
          {icon}
        </span>
        <span className="text-xs">{label}</span>
      </div>
      <div className="text-2xl font-semibold">{value}</div>
      {sub && <div className="text-xs text-muted-foreground">{sub}</div>}
    </div>
  );
}

export default function DesensitizePanel() {
  const { t } = useTranslation();
  const variantSourceLabel: Record<string, string> = {
    self: t("desensitize.variantSelf"),
    synonym: t("desensitize.variantSynonym"),
    folded: t("desensitize.variantFolded"),
  };
  const [dashboard, setDashboard] = useState<DesensitizeDashboard | null>(null);
  const [ruleCategories, setRuleCategories] = useState<DesensitizeRuleCategory[]>([]);
  const [keywords, setKeywords] = useState<DesensitizeKeyword[]>([]);
  const [logs, setLogs] = useState<DesensitizeHitLog[]>([]);
  const [logKw, setLogKw] = useState("");
  const [logPage, setLogPage] = useState(1);
  const PAGE_SIZE = 10;
  const [enabled, setEnabled] = useState(true);
  const [piiEnabled, setPiiEnabled] = useState(true);
  const [keywordEnabled, setKeywordEnabled] = useState(true);
  const [scope, setScope] = useState<"user" | "all">("user");
  const [loading, setLoading] = useState(true);

  // 映射表管理
  const [mappings, setMappings] = useState<DesensitizeMapping[]>([]);
  const [mappingTotal, setMappingTotal] = useState(0);
  const [mappingSearch, setMappingSearch] = useState("");
  const [mappingSession, setMappingSession] = useState("");
  const [mappingOffset, setMappingOffset] = useState(0);
  const [mappingLoading, setMappingLoading] = useState(false);
  const [sessions, setSessions] = useState<string[]>([]);


  // 本地模型管理
  const [modelStatus, setModelStatus] = useState<DesensitizeModelStatus | null>(
    null,
  );
  const [modelLoading, setModelLoading] = useState(false);
  const [mtInput, setMtInput] = useState("");
  const [mtThreshold, setMtThreshold] = useState(0.75);
  const [mtResult, setMtResult] = useState<DesensitizePreview | null>(null);
  const [mtLoading, setMtLoading] = useState(false);
  const [openCats, setOpenCats] = useState<Record<string, boolean>>({});

  // 关键词表单
  const [kwInput, setKwInput] = useState("");
  const [kwMode, setKwMode] = useState("semantic");
  const [kwVariants, setKwVariants] = useState<{ word: string; source: string }[]>([]);

  // 输入关键词时实时展开内置算法待匹配词（防抖 250ms）
  useEffect(() => {
    const kw = kwInput.trim();
    if (!kw) {
      setKwVariants([]);
      return;
    }
    const timer = setTimeout(() => {
      desensitizeApi
        .getKeywordVariants(kw)
        .then(setKwVariants)
        .catch(() => setKwVariants([]));
    }, 250);
    return () => clearTimeout(timer);
  }, [kwInput]);

  // 试算
  const [sandboxOpen, setSandboxOpen] = useState(false);
  const [sbInput, setSbInput] = useState("");
  const [sbResult, setSbResult] = useState<DesensitizePreview | null>(null);
  const [sbLoading, setSbLoading] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [d, rc, k, l, e, scp, pe, ke, st] = await Promise.all([
        desensitizeApi.dashboard(),
        desensitizeApi.listRuleCategories(),
        desensitizeApi.listKeywords(),
        desensitizeApi.queryLogs(500, 0),
        desensitizeApi.getEnabled(),
        desensitizeApi.getScope(),
        desensitizeApi.getPiiEnabled(),
        desensitizeApi.getKeywordEnabled(),
        desensitizeApi.getSemanticThreshold(),
      ]);
      setDashboard(d);
      setRuleCategories(rc);
      setKeywords(k);
      setLogs(l.logs);
      setEnabled(e.enabled);
      setScope(scp.scope);
      setPiiEnabled(pe.enabled);
      setKeywordEnabled(ke.enabled);
      setMtThreshold(st.threshold);
    } catch (err) {
      toast.error(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const typeDistribution = useMemo(() => {
    const total =
      dashboard?.type_distribution.reduce((s, x) => s + x.count, 0) ?? 0;
    return (dashboard?.type_distribution ?? []).map((x) => ({
      ...x,
      pct: total ? Math.round((x.count / total) * 100) : 0,
    }));
  }, [dashboard]);

  const filteredLogs = useMemo(() => {
    const kw = logKw.trim().toLowerCase();
    const filtered = kw
      ? logs.filter((l) =>
          (l.original_masked || "").toLowerCase().includes(kw) ||
          (l.context || "").toLowerCase().includes(kw) ||
          (l.entity_type || "").toLowerCase().includes(kw)
        )
      : logs;
    const start = (logPage - 1) * PAGE_SIZE;
    return filtered.slice(start, start + PAGE_SIZE);
  }, [logs, logKw, logPage]);

  const totalPages = useMemo(() => {
    const kw = logKw.trim().toLowerCase();
    const filtered = kw
      ? logs.filter((l) =>
          (l.original_masked || "").toLowerCase().includes(kw) ||
          (l.context || "").toLowerCase().includes(kw) ||
          (l.entity_type || "").toLowerCase().includes(kw)
        )
      : logs;
    return Math.max(1, Math.ceil(filtered.length / PAGE_SIZE));
  }, [logs, logKw]);

  const handleAddKeyword = async () => {
    const kw = kwInput.trim();
    if (!kw) {
      toast.error(t("desensitize.keywordEmpty"));
      return;
    }
    try {
      await desensitizeApi.addKeyword({
        keyword: kw,
        entityType: "keyword",
        matchMode: kwMode,
        threshold: 0.5,
      });
      toast.success(t("desensitize.keywordAdded"));
      setKwInput("");
      await refresh();
    } catch (err) {
      toast.error(String(err));
    }
  };

  const handleExportTemplate = async () => {
    try {
      const path = await open({
        save: true,
        defaultPath: "desensitize-keywords.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!path) return;
      const res = await desensitizeApi.exportKeywords(path);
      toast.success(t("desensitize.exportTemplateOk", { n: res.count }));
    } catch (err) {
      toast.error(String(err));
    }
  };

  const handleImportTemplate = async () => {
    try {
      const path = await open({
        multiple: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!path) return;
      const res = await desensitizeApi.importKeywords(path);
      toast.success(
        t("desensitize.importTemplateOk", { i: res.imported, s: res.skipped }),
      );
      await refresh();
    } catch (err) {
      toast.error(String(err));
    }
  };

  const handleRemoveKeyword = async (id: number) => {
    try {
      await desensitizeApi.removeKeyword(id);
      toast.success(t("desensitize.keywordRemoved"));
      await refresh();
    } catch (err) {
      toast.error(String(err));
    }
  };

  const handleToggleKeywordSemantic = async (k: DesensitizeKeyword, next: boolean) => {
    const mode = next ? "semantic" : "literal";
    setKeywords((prev) =>
      prev.map((x) => (x.id === k.id ? { ...x, match_mode: mode } : x)),
    );
    try {
      await desensitizeApi.updateKeywordMode(k.id, mode);
      toast.success(next ? t("desensitize.semanticOn") : t("desensitize.semanticOff"));
    } catch (err) {
      setKeywords((prev) =>
        prev.map((x) => (x.id === k.id ? { ...x, match_mode: k.match_mode } : x)),
      );
      toast.error(String(err));
    }
  };

  const handleToggleEnabled = async (next: boolean) => {
    try {
      await desensitizeApi.setEnabled(next);
      setEnabled(next);
      toast.success(next ? t("desensitize.enabledOn") : t("desensitize.enabledOff"));
    } catch (err) {
      toast.error(String(err));
    }
  };

  const handleTogglePii = async (next: boolean) => {
    setPiiEnabled(next);
    try {
      await desensitizeApi.setPiiEnabled(next);
    } catch (err) {
      toast.error(String(err));
      await refresh();
    }
  };

  const handleToggleKeywordStrategy = async (next: boolean) => {
    setKeywordEnabled(next);
    try {
      await desensitizeApi.setKeywordEnabled(next);
    } catch (err) {
      toast.error(String(err));
      await refresh();
    }
  };

  const handleScopeChange = async (next: "user" | "all") => {
    try {
      await desensitizeApi.setScope(next);
      setScope(next);
      toast.success(
        next === "user" ? t("desensitize.scopeUserOn") : t("desensitize.scopeAllOn"),
      );
    } catch (err) {
      toast.error(String(err));
    }
  };

  // ---- 映射表管理 ----
  const loadMappings = useCallback(
    async (reset: boolean, search?: string, session?: string) => {
      const q = search ?? mappingSearch;
      const s = session ?? mappingSession;
      try {
        setMappingLoading(true);
        const off = reset ? 0 : mappingOffset;
        const r = await desensitizeApi.listMappings({
          search: q,
          sessionKey: s,
          limit: 50,
          offset: off,
        });
        if (reset) {
          setMappings(r.items);
        } else {
          setMappings((prev) => [...prev, ...r.items]);
        }
        setMappingTotal(r.total);
        setMappingOffset(off + r.items.length);
        setSessions((prev) => {
          const merged = new Set(prev);
          r.items.forEach((m) => merged.add(m.session_key));
          return Array.from(merged).sort();
        });
      } catch (err) {
        toast.error(String(err));
      } finally {
        setMappingLoading(false);
      }
    },
    [mappingSearch, mappingSession, mappingOffset],
  );

  const searchMappings = () => {
    void loadMappings(true);
  };

  const handleDeleteMapping = async (id: number) => {
    if (!window.confirm(t("desensitize.deleteMappingConfirm"))) return;
    try {
      await desensitizeApi.deleteMapping(id);
      toast.success(t("desensitize.deleted"));
      void loadMappings(true);
      await refresh();
    } catch (err) {
      toast.error(String(err));
    }
  };

  const handleClearAllMappings = async () => {
    if (!window.confirm(t("desensitize.clearAllConfirm"))) return;
    try {
      const r = await desensitizeApi.clearMappings();
      toast.success(t("desensitize.cleared", { n: r.deleted }));
      setMappings([]);
      setMappingTotal(0);
      setMappingOffset(0);
      await refresh();
    } catch (err) {
      toast.error(String(err));
    }
  };

  // ---- 本地模型管理 ----
  const loadModelStatus = useCallback(async () => {
    try {
      setModelLoading(true);
      const r = await desensitizeApi.getModelStatus();
      setModelStatus(r);
    } catch (err) {
      toast.error(String(err));
    } finally {
      setModelLoading(false);
    }
  }, []);

  const handleModelTest = async () => {
    if (!mtInput.trim()) {
      toast.error(t("desensitize.testEmpty"));
      return;
    }
    setMtLoading(true);
    try {
      const r = await desensitizeApi.preview(mtInput, mtThreshold);
      setMtResult(r);
    } catch (err) {
      toast.error(String(err));
    } finally {
      setMtLoading(false);
    }
  };

  // 高亮片段中的命中词 / 占位符
  const highlightContext = (
    text: string,
    key: string,
    cls: string,
  ): React.ReactNode => {
    const i = text.indexOf(key);
    if (i < 0) return text;
    return (
      <>
        {text.slice(0, i)}
        <span className={cls}>{key}</span>
        {text.slice(i + key.length)}
      </>
    );
  };

  const patchCategoryRule = (entityType: string, enabled: boolean) => {
    setRuleCategories((prev) =>
      prev.map((cat) => ({
        ...cat,
        rules: cat.rules.map((r) =>
          r.entity_type === entityType ? { ...r, enabled } : r,
        ),
      })),
    );
  };

  const handleToggleRule = async (rule: DesensitizeRule, next: boolean) => {
    patchCategoryRule(rule.entity_type, next);
    try {
      await desensitizeApi.setRuleEnabled(rule.entity_type, next);
      toast.success(
        next
          ? t("desensitize.ruleEnabledOn", { rule: rule.name })
          : t("desensitize.ruleEnabledOff", { rule: rule.name }),
      );
    } catch (err) {
      patchCategoryRule(rule.entity_type, !next);
      toast.error(String(err));
    }
  };

  const handlePreview = async () => {
    if (!sbInput.trim()) {
      toast.error(t("desensitize.previewEmpty"));
      return;
    }
    setSbLoading(true);
    try {
      const r = await desensitizeApi.preview(sbInput);
      setSbResult(r);
    } catch (err) {
      toast.error(String(err));
    } finally {
      setSbLoading(false);
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center py-24 text-muted-foreground">
        <Loader2 className="w-4 h-4 animate-spin mr-2" />
        {t("desensitize.loading")}
      </div>
    );
  }

  return (
    <div className="px-6 pt-4 flex flex-col flex-1 min-h-0 overflow-hidden">
      <div className="flex-1 overflow-y-auto overflow-x-hidden pb-12 px-1">
        <div className="flex items-center justify-between mb-4">
          <div>
            <h2 className="text-lg font-semibold">
              {t("desensitize.title")}
            </h2>
            <p className="text-xs text-muted-foreground mt-0.5">
              {t("desensitize.subtitle")}
            </p>
          </div>
          <div className="flex items-center gap-2">
            <Badge
              variant={enabled ? "default" : "secondary"}
              className={cn(
                enabled
                  ? "bg-green-100 text-green-700 dark:bg-green-500/15 dark:text-green-400"
                  : "",
              )}
            >
              {enabled ? t("desensitize.on") : t("desensitize.off")}
            </Badge>
            <Switch checked={enabled} onCheckedChange={handleToggleEnabled} />
          </div>
        </div>

        <Tabs
          defaultValue="overview"
          className="w-full"
          onValueChange={(v) => {
            if (v === "mappings") void loadMappings(true);
            if (v === "models") void loadModelStatus();
          }}
        >
          <TabsList>
            <TabsTrigger value="overview">{t("desensitize.overview")}</TabsTrigger>
            <TabsTrigger value="rules">{t("desensitize.rules")}</TabsTrigger>
            <TabsTrigger value="logs">{t("desensitize.logs")}</TabsTrigger>
            <TabsTrigger value="mappings">
              {t("desensitize.mappings")}
            </TabsTrigger>
            <TabsTrigger value="models">
              {t("desensitize.modelManager")}
            </TabsTrigger>
          </TabsList>

          {/* ============ 总览 ============ */}
          <TabsContent value="overview" className="space-y-4">
            <div className="flex flex-wrap gap-3">
              <StatCard
                icon={<Activity className="w-4 h-4" />}
                iconClass="bg-orange-100 text-orange-600 dark:bg-orange-500/15 dark:text-orange-400"
                label={t("desensitize.todayHits")}
                value={dashboard?.today_hits ?? 0}
                sub={t("desensitize.zeroEgress")}
              />
              <StatCard
                icon={<CheckCircle2 className="w-4 h-4" />}
                iconClass="bg-green-100 text-green-600 dark:bg-green-500/15 dark:text-green-400"
                label={t("desensitize.restoreRate")}
                value={dashboard?.total_hits ? `${dashboard.restore_rate}%` : "--"}
                sub={t("desensitize.totalHits", {
                  n: dashboard?.total_hits ?? 0,
                })}
              />
              <StatCard
                icon={<Database className="w-4 h-4" />}
                iconClass="bg-blue-100 text-blue-600 dark:bg-blue-500/15 dark:text-blue-400"
                label={t("desensitize.mappings")}
                value={dashboard?.mapping_count ?? 0}
                sub={t("desensitize.localOnly")}
              />
              <StatCard
                icon={<Users className="w-4 h-4" />}
                iconClass="bg-violet-100 text-violet-600 dark:bg-violet-500/15 dark:text-violet-400"
                label={t("desensitize.activeSessions")}
                value={dashboard?.active_sessions ?? 0}
              />
            </div>

            <div className="flex flex-wrap gap-4">
              {/* 类型分布 */}
              <div className="flex-1 min-w-[280px] rounded-xl border bg-card p-4">
                <div className="flex items-center justify-between mb-3">
                  <h3 className="text-sm font-medium">
                    {t("desensitize.typeDist")}
                  </h3>
                  <span className="text-xs text-muted-foreground">
                    {t("desensitize.today")}
                  </span>
                </div>
                {typeDistribution.length === 0 ? (
                  <p className="text-xs text-muted-foreground py-6 text-center">
                    {t("desensitize.noData")}
                  </p>
                ) : (
                  <div className="space-y-2.5">
                    {typeDistribution.map((x) => (
                      <div
                        key={x.entity_type}
                        className="flex items-center gap-2.5"
                      >
                        <span className="w-20 text-xs truncate">
                          {typeLabel[x.entity_type] ?? x.entity_type}
                        </span>
                        <div className="flex-1 h-2 rounded-full bg-muted overflow-hidden">
                          <div
                            className="h-full rounded-full bg-gradient-to-r from-orange-500 to-orange-400"
                            style={{ width: `${x.pct}%` }}
                          />
                        </div>
                        <span className="w-16 text-right text-[11px] text-muted-foreground">
                          {x.count} · {x.pct}%
                        </span>
                      </div>
                    ))}
                  </div>
                )}
              </div>

              {/* 最近命中 */}
              <div className="flex-1 min-w-[360px] rounded-xl border bg-card p-4">
                <div className="flex items-center justify-between mb-3">
                  <h3 className="text-sm font-medium">
                    {t("desensitize.recentHits")}
                  </h3>
                  <span className="text-xs text-muted-foreground">
                    {t("desensitize.live")}
                  </span>
                </div>
                <div className="overflow-x-auto">
                  <table className="w-full text-xs">
                    <thead>
                      <tr className="text-left text-muted-foreground border-b">
                        <th className="py-1.5 pr-2 font-normal">
                          {t("desensitize.time")}
                        </th>
                        <th className="py-1.5 pr-2 font-normal">
                          {t("desensitize.direction")}
                        </th>
                        <th className="py-1.5 pr-2 font-normal">
                          {t("desensitize.masked")}
                        </th>
                        <th className="py-1.5 pr-2 font-normal">
                          {t("desensitize.type")}
                        </th>
                        <th className="py-1.5 font-normal">
                          {t("desensitize.source")}
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      {(dashboard?.recent_hits ?? []).map((h, i) => (
                        <tr key={i} className="border-b border-muted/50">
                          <td className="py-2 pr-2 font-mono">
                            {fmtTime(h.created_at)}
                          </td>
                          <td className="py-2 pr-2">
                            <Badge
                              variant="secondary"
                              className="font-normal"
                            >
                              {h.direction === "out"
                                ? t("desensitize.dirOut")
                                : t("desensitize.dirIn")}
                            </Badge>
                          </td>
                          <td className="py-2 pr-2">
                            <span className="text-orange-600 dark:text-orange-400">
                              {h.original_masked}
                            </span>
                          </td>
                          <td className="py-2 pr-2">
                            <Badge
                              className={cn(
                                "font-normal",
                                typeColor[h.entity_type] ??
                                  "bg-muted text-muted-foreground",
                              )}
                            >
                              {typeLabel[h.entity_type] ?? h.entity_type}
                            </Badge>
                          </td>
                          <td className="py-2">
                            <Badge
                              variant="secondary"
                              className="font-normal"
                            >
                              {sourceLabel[h.hit_source] ?? h.hit_source}
                            </Badge>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            </div>
          </TabsContent>

          {/* ============ 防护规则（两策略） ============ */}
          <TabsContent value="rules" className="space-y-4">
            {/* 脱敏范围（策略卡片上方） */}
            <div className="rounded-xl border bg-card p-4">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="flex items-center gap-4 flex-wrap">
                  <div>
                    <div className="text-sm font-medium">
                      {t("desensitize.scope")}
                    </div>
                    <div className="text-xs text-muted-foreground">
                      {t("desensitize.scopeDesc")}
                    </div>
                  </div>
                  <Select value={scope} onValueChange={handleScopeChange}>
                    <SelectTrigger className="w-[200px]">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="user">
                        {t("desensitize.scopeUser")}
                      </SelectItem>
                      <SelectItem value="all">
                        {t("desensitize.scopeAll")}
                      </SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              </div>
            </div>

            <div className="grid grid-cols-1 lg:grid-cols-2 gap-4 items-start">
            {/* 策略① 内置 PII */}
            <div className="rounded-xl border bg-card p-4">
              <div className="flex items-start justify-between gap-3 mb-1">
                <div>
                  <h3 className="text-sm font-medium">
                    {t("desensitize.strategy1")}
                  </h3>
                  <p className="text-xs text-muted-foreground mt-0.5">
                    {t("desensitize.strategy1Desc")}
                  </p>
                </div>
                <Switch
                  checked={piiEnabled}
                  onCheckedChange={handleTogglePii}
                  aria-label={t("desensitize.strategy1")}
                />
              </div>
              <div className="mt-3 space-y-2">
                {ruleCategories.map((cat) => {
                  const enabledCount = cat.rules.filter((r) => r.enabled).length;
                  const open = !!openCats[cat.category];
                  return (
                    <Collapsible
                      key={cat.category}
                      open={open}
                      onOpenChange={(o) =>
                        setOpenCats((prev) => ({
                          ...prev,
                          [cat.category]: o,
                        }))
                      }
                    >
                      <CollapsibleTrigger className="w-full">
                        <div className="flex items-center gap-2 rounded-lg border px-3 py-2.5 hover:bg-muted/40 transition-colors w-full">
                          <ShieldCheck className="w-4 h-4 text-muted-foreground shrink-0" />
                          <span className="text-sm">{cat.label}</span>
                          <Badge variant="secondary" className="font-normal">
                            {enabledCount}/{cat.count}{" "}
                            {t("desensitize.on")}
                          </Badge>
                          <span className="ml-auto shrink-0 flex items-center gap-2">
                            <ChevronDown
                              className={cn(
                                "w-4 h-4 text-muted-foreground transition-transform",
                                open && "rotate-180",
                              )}
                            />
                          </span>
                        </div>
                      </CollapsibleTrigger>
                      <CollapsibleContent className="pt-2 space-y-2">
                        {cat.rules.map((r) => (
                          <div
                            key={r.entity_type}
                            className="flex items-center gap-3 rounded-lg border px-3 py-2.5 ml-4"
                          >
                            <div className="min-w-0">
                              <div className="text-sm">
                                {r.name}
                                <span className="ml-1.5 text-[11px] text-muted-foreground font-mono">
                                  {r.entity_type}
                                </span>
                              </div>
                              <div className="text-[11px] text-muted-foreground font-mono truncate">
                                {r.pattern.length > 60
                                  ? r.pattern.slice(0, 60) + "…"
                                  : r.pattern}
                              </div>
                            </div>
                            <span className="ml-auto shrink-0 flex items-center gap-2">
                              <Badge
                                variant={r.enabled ? "default" : "secondary"}
                                className={cn(
                                  "font-normal",
                                  r.enabled
                                    ? "bg-green-100 text-green-700 dark:bg-green-500/15 dark:text-green-400"
                                    : "",
                                )}
                              >
                                {r.enabled
                                  ? t("desensitize.on")
                                  : t("desensitize.off")}
                              </Badge>
                              <Switch
                                checked={r.enabled}
                                onCheckedChange={(next) =>
                                  void handleToggleRule(r, next)
                                }
                                aria-label={r.name}
                              />
                            </span>
                          </div>
                        ))}
                      </CollapsibleContent>
                    </Collapsible>
                  );
                })}
              </div>
            </div>

            {/* 策略② 自定义关键词 */}
            <div className="rounded-xl border bg-card p-4">
              <div className="mb-3 flex items-start justify-between gap-2">
                <div>
                  <h3 className="text-sm font-medium">
                    {t("desensitize.strategy2")}
                  </h3>
                  <p className="text-xs text-muted-foreground mt-0.5">
                    {t("desensitize.strategy2Desc")}
                  </p>
                </div>
                <div className="flex gap-2 shrink-0 items-center">
                  <Switch
                    checked={keywordEnabled}
                    onCheckedChange={handleToggleKeywordStrategy}
                    aria-label={t("desensitize.strategy2")}
                  />
                  <div className="flex gap-2 shrink-0">
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => void handleExportTemplate()}
                  >
                    <Download className="w-4 h-4 mr-1" />
                    {t("desensitize.exportTemplate")}
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => void handleImportTemplate()}
                  >
                    <Upload className="w-4 h-4 mr-1" />
                    {t("desensitize.importTemplate")}
                  </Button>
                  </div>
                </div>
              </div>

              <div className="flex gap-2 mb-3">
                <Input
                  value={kwInput}
                  onChange={(e) => setKwInput(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") void handleAddKeyword();
                  }}
                  placeholder={t("desensitize.keywordPlaceholder")}
                  className="flex-1 min-w-0"
                />
                <Select value={kwMode} onValueChange={setKwMode}>
                  <SelectTrigger className="w-[118px] shrink-0">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="semantic">
                      {t("desensitize.modeSemantic")}
                    </SelectItem>
                    <SelectItem value="literal">
                      {t("desensitize.modeLiteral")}
                    </SelectItem>
                  </SelectContent>
                </Select>
                <Button size="sm" onClick={() => void handleAddKeyword()}>
                  <Plus className="w-4 h-4 mr-1" />
                  {t("desensitize.add")}
                </Button>
              </div>

              {kwVariants.length > 0 && (
                <div className="mb-3 rounded-lg bg-muted/40 px-3 py-2">
                  <div className="text-[11px] text-muted-foreground mb-1.5">
                    {t("desensitize.keywordVariantsTitle")}
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    {kwVariants.map((v, i) => (
                      <span
                        key={i}
                        className="inline-flex items-center gap-1 rounded-md border bg-background px-1.5 py-0.5 text-[11px]"
                      >
                        <span className="font-mono">{v.word}</span>
                        <span className="text-muted-foreground">
                          ·{" "}
                          {variantSourceLabel[v.source] ??
                            t("desensitize.variantOther")}
                        </span>
                      </span>
                    ))}
                  </div>
                </div>
              )}

              {keywords.length === 0 ? (
                <p className="text-xs text-muted-foreground py-6 text-center">
                  {t("desensitize.noKeywords")}
                </p>
              ) : (
                <div className="space-y-2">
                  {keywords.map((k) => (
                    <div
                      key={k.id}
                      className="flex items-center gap-2 rounded-lg border px-3 py-2.5"
                    >
                      <EyeOff className="w-4 h-4 text-violet-500 shrink-0" />
                      <div className="min-w-0">
                        <div className="text-sm">
                          {k.keyword}
                          <span className="ml-1.5 text-[11px] text-muted-foreground font-mono">
                            {k.entity_type}
                          </span>
                        </div>
                        <div className="text-[11px] text-muted-foreground">
                          {k.match_mode === "literal"
                            ? t("desensitize.modeLiteral")
                            : t("desensitize.modeSemantic")}
                        </div>
                      </div>
                      <span className="ml-auto shrink-0 flex items-center gap-2">
                        <span className="text-[11px] text-muted-foreground">
                          {t("desensitize.semantic")}
                        </span>
                        <Switch
                          checked={k.match_mode !== "literal"}
                          onCheckedChange={(next) =>
                            void handleToggleKeywordSemantic(k, next)
                          }
                          aria-label={k.keyword}
                        />
                        <Button
                          variant="ghost"
                          size="icon"
                          className="w-7 h-7 text-muted-foreground hover:text-destructive shrink-0"
                          onClick={() => void handleRemoveKeyword(k.id)}
                          title={t("desensitize.remove")}
                        >
                          <X className="w-3.5 h-3.5" />
                        </Button>
                      </span>
                    </div>
                  ))}
                </div>
              )}

              <div className="mt-3 rounded-lg bg-muted/50 px-3 py-2.5 text-[11px] text-muted-foreground leading-relaxed">
                <b className="text-foreground">
                  {t("desensitize.semanticPrinciple")}
                </b>
                ：{t("desensitize.semanticDesc")}
              </div>

            </div>
            </div>

          </TabsContent>

          {/* ============ 日志 ============ */}
          <TabsContent value="logs" className="space-y-3">
            <div className="rounded-xl border bg-card p-4">
              <div className="flex items-center gap-2 mb-3">
                <input
                  className="flex-1 h-8 rounded-md border bg-background px-2 text-xs"
                  placeholder="搜索被替换的词语..."
                  value={logKw}
                  onChange={(e) => { setLogKw(e.target.value); setLogPage(1); }}
                />
              </div>
              <div className="overflow-x-auto">
                <table className="w-full text-xs">
                  <thead>
                    <tr className="text-left text-muted-foreground border-b">
                      <th className="py-2 pr-3 font-normal">
                        {t("desensitize.time")}
                      </th>
                      <th className="py-2 pr-3 font-normal">
                        {t("desensitize.direction")}
                      </th>
                      <th className="py-2 pr-3 font-normal">
                        原文上下文
                      </th>
                      <th className="py-2 pr-3 font-normal">
                        替换后上下文
                      </th>
                      <th className="py-2 pr-3 font-normal">
                        {t("desensitize.type")}
                      </th>
                      <th className="py-2 pr-3 font-normal">
                        {t("desensitize.source")}
                      </th>
                      <th className="py-2 pr-3 font-normal">
                        {t("desensitize.confidence")}
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {filteredLogs.map((l, i) => (
                      <tr key={i} className="border-b border-muted/50">
                        <td className="py-2 pr-3 font-mono">
                          {fmtTime(l.created_at)}
                        </td>
                        <td className="py-2 pr-3">
                          <Badge
                            variant="secondary"
                            className="font-normal"
                          >
                            {l.direction === "out"
                              ? t("desensitize.dirOut")
                              : t("desensitize.dirIn")}
                          </Badge>
                        </td>
                        <td className="py-2 pr-3 max-w-[260px]">
                          {l.context ? (
                            <HighlightHit
                              text={l.context}
                              hit={l.original_masked}
                            />
                          ) : (
                            <span className="text-orange-600 dark:text-orange-400">
                              {l.original_masked}
                            </span>
                          )}
                        </td>
                        <td className="py-2 pr-3 max-w-[260px]">
                          {l.placeholder_context ? (
                            <HighlightHit
                              text={l.placeholder_context}
                              hit={l.placeholder}
                            />
                          ) : (
                            <span className="font-mono text-muted-foreground">
                              {l.placeholder}
                            </span>
                          )}
                        </td>
                        <td className="py-2 pr-3">
                          <Badge
                            className={cn(
                              "font-normal",
                              typeColor[l.entity_type] ??
                                "bg-muted text-muted-foreground",
                            )}
                          >
                            {typeLabel[l.entity_type] ?? l.entity_type}
                          </Badge>
                        </td>
                        <td className="py-2 pr-3">
                          <Badge variant="secondary" className="font-normal">
                            {sourceLabel[l.hit_source] ?? l.hit_source}
                          </Badge>
                        </td>
                        <td className="py-2 pr-3 font-mono">
                          {l.confidence.toFixed(2)}
                        </td>
                      </tr>
                    ))}
                    {filteredLogs.length === 0 && (
                      <tr>
                        <td
                          colSpan={7}
                          className="py-8 text-center text-muted-foreground"
                        >
                          {t("desensitize.noData")}
                        </td>
                      </tr>
                    )}
                  </tbody>
                </table>
              </div>
              {totalPages > 1 && (
                <div className="flex items-center justify-between mt-3 text-xs">
                  <span className="text-muted-foreground">第 {logPage} / {totalPages} 页 · 共 {filteredLogs.length} 条</span>
                  <div className="flex gap-1">
                    <button className="h-7 px-2 rounded border disabled:opacity-40" disabled={logPage <= 1} onClick={() => setLogPage(logPage - 1)}>上一页</button>
                    <button className="h-7 px-2 rounded border disabled:opacity-40" disabled={logPage >= totalPages} onClick={() => setLogPage(logPage + 1)}>下一页</button>
                  </div>
                </div>
              )}
            </div>
          </TabsContent>

          {/* ============ 映射表管理 ============ */}
          <TabsContent value="mappings" className="space-y-3">
            {/* 映射规则 */}
            <div className="rounded-xl border bg-card p-4">
              <h3 className="text-sm font-medium mb-2">
                {t("desensitize.mappingRules")}
              </h3>
              <p className="text-xs text-muted-foreground mt-1 leading-relaxed whitespace-pre-line">
                {t("desensitize.placeholderFormat")}
              </p>
              <p className="text-xs text-muted-foreground mt-1 leading-relaxed">
                {t("desensitize.mappingRuleNotes")}
              </p>
            </div>

            {/* 已生成映射 */}
            <div className="rounded-xl border bg-card p-4">
              <div className="flex items-center justify-between mb-3 flex-wrap gap-2">
                <h3 className="text-sm font-medium">
                  {t("desensitize.generatedMappings")}
                  <span className="text-muted-foreground font-normal ml-2">
                    {t("desensitize.mappingTotal", { n: mappingTotal })}
                  </span>
                </h3>
                <div className="flex items-center gap-2 flex-wrap">
                  <Input
                    value={mappingSearch}
                    onChange={(e) => setMappingSearch(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") searchMappings();
                    }}
                    placeholder={t("desensitize.searchMapping")}
                    className="w-[200px] h-8 text-xs"
                  />
                  <Select
                    value={mappingSession || "__all__"}
                    onValueChange={(v) => {
                      const next = v === "__all__" ? "" : v;
                      setMappingSession(next);
                      void loadMappings(true, mappingSearch, next);
                    }}
                  >
                    <SelectTrigger className="w-[130px] h-8 text-xs">
                      <SelectValue placeholder={t("desensitize.allSessions")} />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="__all__">
                        {t("desensitize.allSessions")}
                      </SelectItem>
                      {sessions.map((ss) => (
                        <SelectItem key={ss} value={ss}>
                          {ss.slice(0, 12)}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="text-muted-foreground"
                    onClick={searchMappings}
                  >
                    <Search className="w-3.5 h-3.5 mr-1" />
                    {t("desensitize.search")}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="text-muted-foreground"
                    onClick={() => void loadMappings(true)}
                    disabled={mappingLoading}
                  >
                    <RefreshCw
                      className={cn(
                        "w-3.5 h-3.5 mr-1",
                        mappingLoading && "animate-spin",
                      )}
                    />
                    {t("desensitize.refresh")}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="text-muted-foreground hover:text-destructive"
                    onClick={() => void handleClearAllMappings()}
                  >
                    <Trash2 className="w-3.5 h-3.5 mr-1" />
                    {t("desensitize.clearAll")}
                  </Button>
                </div>
              </div>
              <div className="overflow-x-auto">
                <table className="w-full text-xs">
                  <thead>
                    <tr className="text-left text-muted-foreground border-b">
                      <th className="py-2 pr-3 font-normal">
                        {t("desensitize.placeholder")}
                      </th>
                      <th className="py-2 pr-3 font-normal">
                        {t("desensitize.original")}
                      </th>
                      <th className="py-2 pr-3 font-normal">
                        {t("desensitize.type")}
                      </th>
                      <th className="py-2 pr-3 font-normal">
                        {t("desensitize.source")}
                      </th>
                      <th className="py-2 pr-3 font-normal">
                        {t("desensitize.session")}
                      </th>
                      <th className="py-2 pr-3 font-normal">
                        {t("desensitize.time")}
                      </th>
                      <th className="py-2 font-normal">
                        {t("desensitize.action")}
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {mappings.map((m) => (
                      <tr key={m.id} className="border-b border-muted/50">
                        <td className="py-2 pr-3 font-mono text-muted-foreground">
                          {m.placeholder}
                        </td>
                        <td
                          className="py-2 pr-3 max-w-[260px] truncate"
                          title={m.original}
                        >
                          <span className="text-orange-600 dark:text-orange-400">
                            {m.original}
                          </span>
                        </td>
                        <td className="py-2 pr-3">
                          <Badge
                            className={cn(
                              "font-normal",
                              typeColor[m.entity_type] ??
                                "bg-muted text-muted-foreground",
                            )}
                          >
                            {typeLabel[m.entity_type] ?? m.entity_type}
                          </Badge>
                        </td>
                        <td className="py-2 pr-3">
                          <Badge variant="secondary" className="font-normal">
                            {sourceLabel[m.hit_source] ?? m.hit_source}
                          </Badge>
                        </td>
                        <td className="py-2 pr-3 font-mono text-muted-foreground">
                          {m.session_key.slice(0, 12)}
                        </td>
                        <td className="py-2 pr-3 font-mono">
                          {fmtDateTime(m.created_at)}
                        </td>
                        <td className="py-2">
                          <Button
                            variant="ghost"
                            size="icon"
                            className="w-7 h-7 text-muted-foreground hover:text-destructive"
                            onClick={() => void handleDeleteMapping(m.id)}
                            title={t("desensitize.delete")}
                          >
                            <X className="w-3.5 h-3.5" />
                          </Button>
                        </td>
                      </tr>
                    ))}
                    {mappings.length === 0 && !mappingLoading && (
                      <tr>
                        <td
                          colSpan={7}
                          className="py-8 text-center text-muted-foreground"
                        >
                          {t("desensitize.noMappings")}
                        </td>
                      </tr>
                    )}
                  </tbody>
                </table>
              </div>
              {mappingTotal > mappings.length && (
                <div className="mt-3 text-center">
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => void loadMappings(false)}
                    disabled={mappingLoading}
                  >
                    {mappingLoading ? (
                      <Loader2 className="w-3.5 h-3.5 animate-spin mr-1" />
                    ) : null}
                    {t("desensitize.loadMore")}（{mappings.length}/
                    {mappingTotal}）
                  </Button>
                </div>
              )}
            </div>
          </TabsContent>

          {/* ============ 本地模型管理 ============ */}
          <TabsContent value="models" className="space-y-3">
            {/* 模型状态 */}
            <div className="rounded-xl border bg-card p-4">
              <div className="flex items-center justify-between mb-3 flex-wrap gap-2">
                <h3 className="text-sm font-medium">
                  {t("desensitize.modelStatus")}
                </h3>
                <Button
                  variant="ghost"
                  size="sm"
                  className="text-muted-foreground"
                  onClick={() => void loadModelStatus()}
                  disabled={modelLoading}
                >
                  <RefreshCw
                    className={cn(
                      "w-3.5 h-3.5 mr-1",
                      modelLoading && "animate-spin",
                    )}
                  />
                  {t("desensitize.refresh")}
                </Button>
              </div>
              <div className="flex items-center justify-between rounded-lg border px-3 py-2.5 max-w-md">
                <div className="text-xs text-muted-foreground">
                  {t("desensitize.modelStatusShort")}
                </div>
                <Badge
                  variant={modelStatus?.embedding.ready ? "default" : "destructive"}
                  className={cn(
                    "font-normal",
                    modelStatus?.embedding.ready
                      ? "bg-green-100 text-green-700 dark:bg-green-500/15 dark:text-green-400"
                      : "",
                  )}
                >
                  {modelLoading && !modelStatus
                    ? t("desensitize.modelChecking")
                    : modelStatus?.embedding.ready
                      ? t("desensitize.modelReady")
                      : t("desensitize.modelNotReady")}
                </Badge>
              </div>
              {modelStatus?.embedding.error && (
                <p className="text-xs text-destructive mt-2 break-all">
                  {modelStatus.embedding.error}
                </p>
              )}
              <p className="text-xs text-muted-foreground mt-3 leading-relaxed">
                {t("desensitize.modelDesc")}
              </p>
            </div>

            {/* 文字测试 */}
            <div className="rounded-xl border bg-card p-4">
              <h3 className="text-sm font-medium mb-1">
                {t("desensitize.modelTest")}
              </h3>
              <p className="text-xs text-muted-foreground mb-3">
                {t("desensitize.modelTestHint")}
              </p>
              <div className="flex flex-wrap gap-4">
                <div className="flex-1 min-w-[280px]">
                  <Textarea
                    value={mtInput}
                    onChange={(e) => setMtInput(e.target.value)}
                    rows={10}
                    placeholder={t("desensitize.testInputPlaceholder")}
                    className="font-mono text-xs"
                  />
                  <div className="mt-3 rounded-lg border px-3 py-2.5">
                    <div className="flex items-center justify-between mb-1.5">
                      <span className="text-xs text-muted-foreground">
                        {t("desensitize.threshold")}
                      </span>
                      <span className="text-xs font-mono">
                        {mtThreshold.toFixed(2)}
                      </span>
                    </div>
                    <input
                      type="range"
                      min={0.3}
                      max={0.9}
                      step={0.05}
                      value={mtThreshold}
                      onChange={(e) => {
                        const v = Number(e.target.value);
                        setMtThreshold(v);
                        void desensitizeApi
                          .setSemanticThreshold(v)
                          .catch((err) => toast.error(String(err)));
                      }}
                      className="w-full"
                    />
                    <div className="flex justify-between text-[10px] text-muted-foreground mt-1">
                      <span>0.30</span>
                      <span>0.90</span>
                    </div>
                  </div>
                  <div className="flex gap-2 mt-3">
                    <Button
                      size="sm"
                      onClick={() => void handleModelTest()}
                      disabled={mtLoading}
                    >
                      {mtLoading ? (
                        <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
                      ) : (
                        <Play className="w-3.5 h-3.5 mr-1.5" />
                      )}
                      {t("desensitize.testBtn")}
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => {
                        setMtInput("");
                        setMtResult(null);
                      }}
                    >
                      {t("desensitize.clear")}
                    </Button>
                  </div>
                </div>
                <div className="flex-1 min-w-[320px]">
                  <div className="text-[11px] text-muted-foreground mb-1.5">
                    {t("desensitize.afterPlaceholder")}
                  </div>
                  {mtResult && mtResult.hits.length > 0 ? (
                    <div className="rounded-lg border overflow-x-auto">
                      <table className="w-full text-xs">
                        <thead>
                          <tr className="bg-muted/50">
                            <th className="text-left px-3 py-2 font-medium text-muted-foreground whitespace-nowrap">
                              {t("desensitize.mtColContext")}
                            </th>
                            <th className="text-left px-3 py-2 font-medium text-muted-foreground whitespace-nowrap">
                              {t("desensitize.mtColMasked")}
                            </th>
                            <th className="text-left px-3 py-2 font-medium text-muted-foreground whitespace-nowrap">
                              {t("desensitize.mtColType")}
                            </th>
                            <th className="text-left px-3 py-2 font-medium text-muted-foreground whitespace-nowrap">
                              {t("desensitize.mtColSource")}
                            </th>
                            <th className="text-left px-3 py-2 font-medium text-muted-foreground whitespace-nowrap">
                              {t("desensitize.mtColConf")}
                            </th>
                          </tr>
                        </thead>
                        <tbody>
                          {mtResult.hits.map((h, i) => (
                            <tr
                              key={i}
                              className="border-t align-top hover:bg-muted/20"
                            >
                              <td className="px-3 py-2 font-mono leading-relaxed break-all max-w-[240px]">
                                {highlightContext(
                                  h.context,
                                  h.original,
                                  "text-orange-600 dark:text-orange-400 line-through decoration-1",
                                )}
                              </td>
                              <td className="px-3 py-2 font-mono leading-relaxed break-all max-w-[240px]">
                                {highlightContext(
                                  h.placeholder_context,
                                  h.placeholder,
                                  "text-green-600 dark:text-green-400 font-medium",
                                )}
                              </td>
                              <td className="px-3 py-2 whitespace-nowrap">
                                <Badge
                                  className={cn(
                                    "font-normal",
                                    typeColor[h.entity_type] ??
                                      "bg-muted text-muted-foreground",
                                  )}
                                >
                                  {typeLabel[h.entity_type] ?? h.entity_type}
                                </Badge>
                              </td>
                              <td className="px-3 py-2 whitespace-nowrap text-muted-foreground">
                                {sourceLabel[h.source] ?? h.source}
                              </td>
                              <td className="px-3 py-2 whitespace-nowrap text-muted-foreground">
                                {h.confidence.toFixed(2)}
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  ) : (
                    <div className="rounded-lg border bg-muted/30 p-3 min-h-[120px] whitespace-pre-wrap break-all font-mono text-xs text-muted-foreground">
                      {mtResult
                        ? t("desensitize.previewNoHit")
                        : t("desensitize.previewHint")}
                    </div>
                  )}
                </div>
              </div>
            </div>
          </TabsContent>


        </Tabs>
      </div>

      {/* 试算弹层 */}
      <Dialog open={sandboxOpen} onOpenChange={setSandboxOpen}>
        <DialogContent className="max-w-[860px] relative">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <ShieldCheck className="w-4 h-4" />
              {t("desensitize.previewTitle")}
            </DialogTitle>
            <DialogClose
              className="absolute right-4 top-4 rounded-md p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
              title={t("common.close", { defaultValue: "关闭" })}
            >
              <X className="w-4 h-4" />
            </DialogClose>
          </DialogHeader>
          <div className="flex flex-wrap gap-4">
            <div className="flex-1 min-w-[280px]">
              <div className="text-[11px] text-muted-foreground mb-1.5">
                {t("desensitize.originalText")}
              </div>
              <Textarea
                value={sbInput}
                onChange={(e) => setSbInput(e.target.value)}
                rows={8}
                placeholder={t("desensitize.previewPlaceholder")}
                className="font-mono text-xs"
              />
              <div className="flex gap-2 mt-2">
                <Button
                  size="sm"
                  onClick={() => void handlePreview()}
                  disabled={sbLoading}
                >
                  {sbLoading && <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />}
                  {t("desensitize.previewRun")}
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => {
                    setSbInput("");
                    setSbResult(null);
                  }}
                >
                  {t("desensitize.clear")}
                </Button>
              </div>
            </div>
            <div className="flex-1 min-w-[280px]">
              <div className="text-[11px] text-muted-foreground mb-1.5">
                {t("desensitize.afterMask")}
              </div>
              <div className="rounded-lg border bg-muted/30 p-3 min-h-[190px] whitespace-pre-wrap break-all font-mono text-xs">
                {sbResult?.masked ?? t("desensitize.previewHint")}
              </div>
              {sbResult && sbResult.hits.length > 0 && (
                <div className="mt-2 text-[11px] space-y-1">
                  {t("desensitize.previewHit", {
                    n: sbResult.hits.length,
                  })}
                  {sbResult.hits.map((h, i) => (
                    <div key={i} className="flex items-center gap-1.5">
                      <Badge
                        className={cn(
                          "font-normal",
                          typeColor[h.entity_type] ??
                            "bg-muted text-muted-foreground",
                        )}
                      >
                        {typeLabel[h.entity_type] ?? h.entity_type}
                      </Badge>
                      <span className="text-muted-foreground">
                        {sourceLabel[h.source] ?? h.source} ·{" "}
                        {h.confidence.toFixed(2)}
                      </span>
                      <span className="text-orange-600 dark:text-orange-400">
                        {h.original_masked}
                      </span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
