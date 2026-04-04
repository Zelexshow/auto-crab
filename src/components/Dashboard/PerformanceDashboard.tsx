import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Activity, Zap, Clock, Server, RefreshCw, Loader2,
  ArrowUpRight, ArrowDownRight, Minus, Wallet,
} from "lucide-react";

interface PerfEvent {
  event_type: string;
  label: string;
  duration_ms: number;
  timestamp: string;
}

interface PerfSummary {
  remote_chat_count: number;
  remote_chat_avg_ms: number;
  desktop_chat_count: number;
  desktop_chat_avg_ms: number;
  enrich_count: number;
  enrich_avg_ms: number;
  tool_call_count: number;
  tool_call_avg_ms: number;
  scheduled_task_count: number;
  scheduled_task_avg_ms: number;
}

interface McpServer {
  0: string;
  1: number;
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
  return `${(ms / 60000).toFixed(1)}m`;
}

function StatCard({ icon, label, value, sub, trend }: {
  icon: React.ReactNode;
  label: string;
  value: string;
  sub?: string;
  trend?: "up" | "down" | "flat";
}) {
  return (
    <div
      className="stat-card rounded-xl"
      style={{
        background: "var(--bg-secondary)",
        border: "1px solid var(--border)",
        boxShadow: "var(--shadow-sm)",
        padding: "16px 18px",
        minWidth: 0,
      }}
    >
      <div className="flex items-center gap-3">
        <div className="w-9 h-9 rounded-lg flex items-center justify-center shrink-0" style={{ background: "var(--accent-light)" }}>
          {icon}
        </div>
        <div style={{ minWidth: 0 }}>
          <div className="text-2xl font-bold tabular-nums leading-tight" style={{ color: "var(--text-primary)" }}>{value}</div>
          <div className="text-xs mt-0.5" style={{ color: "var(--text-muted)" }}>{label}</div>
        </div>
        {trend && (
          <span className="ml-auto shrink-0" style={{ color: trend === "up" ? "var(--success)" : trend === "down" ? "var(--danger)" : "var(--text-muted)" }}>
            {trend === "up" ? <ArrowUpRight size={14} /> : trend === "down" ? <ArrowDownRight size={14} /> : <Minus size={14} />}
          </span>
        )}
      </div>
      {sub && <div className="text-[11px] mt-2 pl-12" style={{ color: "var(--text-muted)" }}>{sub}</div>}
    </div>
  );
}

function EventTypeIcon({ type: t }: { type: string }) {
  const style = { width: 6, height: 6, borderRadius: "50%", flexShrink: 0 };
  const colors: Record<string, string> = {
    remote_chat: "var(--accent)",
    desktop_chat: "#60a5fa",
    enrich: "var(--warning)",
    tool_call: "var(--success)",
    scheduled_task: "#a78bfa",
  };
  return <div style={{ ...style, background: colors[t] || "var(--text-muted)" }} />;
}

const EVENT_TYPE_LABELS: Record<string, string> = {
  remote_chat: "飞书对话",
  desktop_chat: "桌面对话",
  enrich: "数据预取",
  tool_call: "工具调用",
  scheduled_task: "定时任务",
};

const BALANCE_PROVIDERS: { key: string; name: string; type: "currency" | "quota" | "info" }[] = [
  { key: "deepseek", name: "DeepSeek", type: "currency" },
  { key: "moonshot", name: "月之暗面 Kimi", type: "currency" },
  { key: "tavily", name: "Tavily", type: "quota" },
  { key: "serpapi", name: "SerpApi", type: "quota" },
  { key: "brave", name: "Brave Search", type: "quota" },
  { key: "dashscope", name: "通义千问", type: "info" },
];

function BalanceItem({ name, data, type }: { name: string; data: any; type: "currency" | "quota" | "info" }) {
  const isAvailable = data.available === true;
  const hasError = !!data.error;
  const reason = data.reason as string | undefined;

  const statusColor = !isAvailable
    ? "var(--text-muted)"
    : type === "currency"
      ? parseFloat(data.total || "0") > 5 ? "#22c55e" : parseFloat(data.total || "0") > 0 ? "#f59e0b" : "#ef4444"
      : (data.remaining ?? 0) > 50 ? "#22c55e" : (data.remaining ?? 0) > 10 ? "#f59e0b" : "#ef4444";

  const quotaPct = type === "quota" && isAvailable && data.limit > 0
    ? Math.round(((data.limit - (data.remaining ?? 0)) / data.limit) * 100)
    : 0;

  return (
    <div className="rounded-lg" style={{ background: "var(--bg-tertiary)", padding: "14px 16px" }}>
      <div className="flex items-center justify-between mb-1.5">
        <span className="text-[12px] font-medium" style={{ color: "var(--text-primary)" }}>{name}</span>
        <span className="inline-block w-2 h-2 rounded-full" style={{ background: isAvailable ? statusColor : "var(--border)" }} />
      </div>
      {!isAvailable && !hasError && reason && (
        <p className="text-[11px]" style={{ color: "var(--text-muted)" }}>{reason}</p>
      )}
      {hasError && (
        <p className="text-[11px]" style={{ color: "#ef4444" }} title={data.error}>
          {String(data.error).length > 40 ? String(data.error).slice(0, 40) + "..." : data.error}
        </p>
      )}
      {isAvailable && type === "currency" && (
        <>
          <div className="flex items-baseline gap-1.5">
            <span className="text-[20px] font-bold tabular-nums" style={{ color: statusColor }}>{data.total}</span>
            <span className="text-[11px]" style={{ color: "var(--text-muted)" }}>{data.currency}</span>
          </div>
          {data.detail && <p className="text-[10px] mt-1" style={{ color: "var(--text-muted)" }}>{data.detail}</p>}
        </>
      )}
      {isAvailable && type === "quota" && !data.is_per_second && (
        <>
          <div className="flex items-baseline gap-1.5">
            <span className="text-[20px] font-bold tabular-nums" style={{ color: statusColor }}>{data.remaining ?? "?"}</span>
            <span className="text-[11px]" style={{ color: "var(--text-muted)" }}>/ {data.limit} 剩余</span>
          </div>
          {data.limit > 0 && (
            <div className="mt-2 h-1.5 rounded-full overflow-hidden" style={{ background: "var(--bg-primary)" }}>
              <div className="h-full rounded-full" style={{
                width: `${quotaPct}%`,
                background: quotaPct < 70 ? "#22c55e" : quotaPct < 90 ? "#f59e0b" : "#ef4444",
              }} />
            </div>
          )}
          {data.plan && <p className="text-[10px] mt-1" style={{ color: "var(--text-muted)" }}>{data.plan}</p>}
          {(data.source === "cached" || data.source === "heartbeat") && data.cached_at && (
            <p className="text-[10px] mt-0.5" style={{ color: "var(--text-muted)" }}>更新于 {data.cached_at}</p>
          )}
        </>
      )}
      {isAvailable && type === "quota" && data.is_per_second && (
        <>
          <div className="flex items-baseline gap-1.5">
            <span className="text-[20px] font-bold tabular-nums" style={{ color: "#22c55e" }}>{data.limit}</span>
            <span className="text-[11px]" style={{ color: "var(--text-muted)" }}>req/s 容量</span>
          </div>
          {data.plan && <p className="text-[10px] mt-1" style={{ color: "var(--text-muted)" }}>{data.plan}</p>}
          {data.cached_at && (
            <p className="text-[10px] mt-0.5" style={{ color: "var(--text-muted)" }}>更新于 {data.cached_at}</p>
          )}
        </>
      )}
    </div>
  );
}

export function PerformanceDashboard() {
  const [summary, setSummary] = useState<PerfSummary | null>(null);
  const [events, setEvents] = useState<PerfEvent[]>([]);
  const [mcpServers, setMcpServers] = useState<McpServer[]>([]);
  const [balances, setBalances] = useState<Record<string, any> | null>(null);
  const [balanceLoading, setBalanceLoading] = useState(false);
  const [loading, setLoading] = useState(true);
  const [lastRefresh, setLastRefresh] = useState<Date>(new Date());

  const fetchBalances = useCallback(async () => {
    setBalanceLoading(true);
    try {
      const res = await invoke<{ success: boolean; data?: Record<string, any> }>("query_provider_balances");
      if (res.success && res.data) setBalances(res.data);
    } catch (e) {
      console.error("Failed to fetch balances:", e);
    }
    setBalanceLoading(false);
  }, []);

  const fetchData = useCallback(async () => {
    setLoading(true);
    try {
      const [perfRes, mcpRes] = await Promise.all([
        invoke<{ success: boolean; data?: any }>("get_perf_metrics"),
        invoke<{ success: boolean; data?: any }>("get_mcp_status"),
      ]);
      if (perfRes.success && perfRes.data) {
        setSummary(perfRes.data.summary);
        setEvents(perfRes.data.recent_events || []);
      }
      if (mcpRes.success && mcpRes.data) {
        setMcpServers(mcpRes.data);
      }
      setLastRefresh(new Date());
    } catch (e) {
      console.error("Failed to fetch perf metrics:", e);
    }
    setLoading(false);
  }, []);

  useEffect(() => {
    fetchData();
    fetchBalances();
    const interval = setInterval(fetchData, 30000);
    return () => clearInterval(interval);
  }, [fetchData, fetchBalances]);

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <header
        className="flex items-center justify-between px-6 h-12 border-b shrink-0"
        style={{ borderColor: "var(--border)", background: "var(--bg-secondary)" }}
      >
        <div className="flex items-center gap-2">
          <Activity size={16} style={{ color: "var(--accent)" }} />
          <h1 className="font-semibold text-sm">性能监控</h1>
          <span className="text-[11px] px-1.5 py-0.5 rounded" style={{ background: "var(--bg-tertiary)", color: "var(--text-muted)" }}>
            实时
          </span>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-[11px]" style={{ color: "var(--text-muted)" }}>
            {lastRefresh.toLocaleTimeString("zh-CN")} 更新
          </span>
          <button
            onClick={fetchData}
            disabled={loading}
            className="w-7 h-7 rounded-lg flex items-center justify-center transition-colors"
            style={{ background: "var(--bg-tertiary)", color: "var(--text-secondary)" }}
          >
            {loading ? <Loader2 size={13} className="animate-spin" /> : <RefreshCw size={13} />}
          </button>
        </div>
      </header>

      <div className="flex-1 overflow-y-auto" style={{ background: "var(--bg-primary)", padding: "24px 28px" }}>
        <div style={{ display: "flex", flexDirection: "column", gap: 20 }}>

        {/* Stats grid */}
        <div className="grid gap-4" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))" }}>
          <StatCard
            icon={<Zap size={16} style={{ color: "var(--accent)" }} />}
            label="飞书对话"
            value={summary ? String(summary.remote_chat_count) : "—"}
            sub={summary && summary.remote_chat_avg_ms > 0 ? `平均 ${formatDuration(summary.remote_chat_avg_ms)}` : undefined}
          />
          <StatCard
            icon={<Zap size={16} style={{ color: "#60a5fa" }} />}
            label="桌面对话"
            value={summary ? String(summary.desktop_chat_count) : "—"}
            sub={summary && summary.desktop_chat_avg_ms > 0 ? `平均 ${formatDuration(summary.desktop_chat_avg_ms)}` : undefined}
          />
          <StatCard
            icon={<Activity size={16} style={{ color: "var(--success)" }} />}
            label="工具调用"
            value={summary ? String(summary.tool_call_count) : "—"}
            sub={summary && summary.tool_call_avg_ms > 0 ? `平均 ${formatDuration(summary.tool_call_avg_ms)}` : undefined}
          />
          <StatCard
            icon={<Clock size={16} style={{ color: "var(--warning)" }} />}
            label="数据预取"
            value={summary ? String(summary.enrich_count) : "—"}
            sub={summary && summary.enrich_avg_ms > 0 ? `平均 ${formatDuration(summary.enrich_avg_ms)}` : undefined}
          />
          <StatCard
            icon={<Server size={16} style={{ color: "#a78bfa" }} />}
            label="定时任务"
            value={summary ? String(summary.scheduled_task_count) : "—"}
            sub={summary && summary.scheduled_task_avg_ms > 0 ? `平均 ${formatDuration(summary.scheduled_task_avg_ms)}` : undefined}
          />
        </div>

        {/* Provider Balances */}
        <div
          className="rounded-xl"
          style={{ background: "var(--bg-secondary)", border: "1px solid var(--border)", padding: "20px 24px" }}
        >
          <div className="flex items-center justify-between" style={{ marginBottom: 16 }}>
            <div className="flex items-center gap-2">
              <Wallet size={14} style={{ color: "var(--accent)" }} />
              <h3 className="text-sm font-semibold" style={{ color: "var(--text-primary)" }}>API 余额与用量</h3>
            </div>
            <button
              onClick={fetchBalances}
              disabled={balanceLoading}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-[11px] transition-colors"
              style={{ background: "var(--bg-tertiary)", color: "var(--text-secondary)" }}
            >
              {balanceLoading ? <Loader2 size={11} className="animate-spin" /> : <RefreshCw size={11} />}
              {balanceLoading ? "查询中..." : "查询余额"}
            </button>
          </div>
          {balances ? (
            <div className="grid grid-cols-3 gap-3">
              {BALANCE_PROVIDERS.map(({ key, name, type }) => {
                const d = balances[key];
                if (!d) return null;
                return <BalanceItem key={key} name={name} data={d} type={type} />;
              })}
            </div>
          ) : (
            <div className="flex items-center justify-center py-5 gap-2">
              <Wallet size={16} style={{ color: "var(--text-muted)", opacity: 0.4 }} />
              <span className="text-xs" style={{ color: "var(--text-muted)" }}>
                点击「查询余额」获取各服务商实时余额和用量
              </span>
            </div>
          )}
        </div>

        {/* MCP Status - full width */}
        <div
          className="rounded-xl"
          style={{ background: "var(--bg-secondary)", border: "1px solid var(--border)", padding: "20px 24px" }}
        >
          <h3 className="text-sm font-semibold" style={{ color: "var(--text-primary)", marginBottom: 12 }}>MCP 服务器</h3>
          {mcpServers.length > 0 ? (
            <div className="grid grid-cols-3 gap-2.5">
              {mcpServers.map((srv, i) => (
                <div key={i} className="flex items-center justify-between rounded-lg" style={{ background: "var(--bg-tertiary)", padding: "10px 14px" }}>
                  <div className="flex items-center gap-2">
                    <div className="w-2 h-2 rounded-full" style={{ background: "var(--success)" }} />
                    <span className="text-xs font-medium" style={{ color: "var(--text-primary)" }}>{srv[0]}</span>
                  </div>
                  <span className="text-[11px] tabular-nums" style={{ color: "var(--text-muted)" }}>
                    {srv[1]} 工具
                  </span>
                </div>
              ))}
            </div>
          ) : (
            <div className="flex items-center justify-center py-4 gap-2">
              <Server size={18} style={{ color: "var(--text-muted)", opacity: 0.4 }} />
              <span className="text-xs" style={{ color: "var(--text-muted)" }}>未连接 MCP 服务器</span>
            </div>
          )}
        </div>

        {/* Recent events timeline */}
        <div
          className="rounded-xl"
          style={{ background: "var(--bg-secondary)", border: "1px solid var(--border)", padding: "20px 24px" }}
        >
          <div className="flex items-center justify-between" style={{ marginBottom: 14 }}>
            <h3 className="text-sm font-semibold" style={{ color: "var(--text-primary)" }}>最近事件</h3>
            <span className="text-[10px] px-1.5 py-0.5 rounded" style={{ background: "var(--bg-tertiary)", color: "var(--text-muted)" }}>
              最近 50 条
            </span>
          </div>
          {events.length > 0 ? (
            <div className="space-y-0.5">
              <div className="flex items-center gap-3 px-2 py-1.5 text-[10px] font-medium" style={{ color: "var(--text-muted)" }}>
                <span className="w-[70px]">时间</span>
                <span className="w-20">类型</span>
                <span className="flex-1">详情</span>
                <span className="w-[70px] text-right">耗时</span>
                <span className="w-28">耗时分布</span>
              </div>
              {events.map((ev, i) => {
                const maxMs = Math.max(...events.map(e => e.duration_ms), 1);
                const pct = Math.min((ev.duration_ms / maxMs) * 100, 100);
                const barColor = ev.duration_ms > 30000 ? "var(--danger)"
                  : ev.duration_ms > 10000 ? "var(--warning)" : "var(--success)";
                return (
                  <div
                    key={i}
                    className="flex items-center gap-3 px-2 py-2 rounded-md transition-colors"
                    style={{ background: i % 2 === 0 ? "transparent" : "var(--bg-tertiary)" }}
                  >
                    <span className="text-[11px] tabular-nums w-[70px] shrink-0" style={{ color: "var(--text-muted)" }}>
                      {ev.timestamp.split(" ")[1] || ev.timestamp}
                    </span>
                    <span className="flex items-center gap-1.5 w-20 shrink-0">
                      <EventTypeIcon type={ev.event_type} />
                      <span className="text-[11px]" style={{ color: "var(--text-secondary)" }}>
                        {EVENT_TYPE_LABELS[ev.event_type] || ev.event_type}
                      </span>
                    </span>
                    <span className="flex-1 text-[11px] truncate" style={{ color: "var(--text-primary)" }}>
                      {ev.label}
                    </span>
                    <span
                      className="text-[11px] tabular-nums w-[70px] text-right font-medium shrink-0"
                      style={{ color: ev.duration_ms > 30000 ? "var(--danger)" : ev.duration_ms > 10000 ? "var(--warning)" : "var(--text-primary)" }}
                    >
                      {formatDuration(ev.duration_ms)}
                    </span>
                    <div className="w-28 shrink-0">
                      <div className="h-1.5 rounded-full overflow-hidden" style={{ background: "var(--bg-primary)" }}>
                        <div className="h-full rounded-full perf-bar" style={{ width: `${pct}%`, background: barColor }} />
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          ) : (
            <div className="flex flex-col items-center justify-center py-8 gap-2">
              <Activity size={24} style={{ color: "var(--text-muted)", opacity: 0.3 }} />
              <span className="text-xs" style={{ color: "var(--text-muted)" }}>暂无性能事件记录</span>
              <span className="text-[10px]" style={{ color: "var(--text-muted)" }}>当有对话、工具调用或定时任务执行后，数据将自动出现</span>
            </div>
          )}
        </div>

        </div>
      </div>
    </div>
  );
}
