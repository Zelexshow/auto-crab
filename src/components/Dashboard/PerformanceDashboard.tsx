import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Activity, Zap, Clock, Server, RefreshCw, Loader2,
  ArrowUpRight, ArrowDownRight, Minus,
} from "lucide-react";

interface PerfEvent {
  event_type: string;
  label: string;
  duration_ms: number;
  timestamp: string;
}

interface PerfSummary {
  chat_count: number;
  chat_avg_ms: number;
  enrich_count: number;
  enrich_avg_ms: number;
  tool_call_count: number;
  scheduled_task_count: number;
}

interface SearchStats {
  serpapi: { used: number; quota: number; remaining: number };
  brave: { used: number; quota: number; remaining: number };
  tavily: { used: number; quota: number; remaining: number };
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
      className="stat-card rounded-xl p-4"
      style={{
        background: "var(--bg-secondary)",
        border: "1px solid var(--border)",
        boxShadow: "var(--shadow-sm)",
      }}
    >
      <div className="flex items-center justify-between mb-2">
        <div className="w-8 h-8 rounded-lg flex items-center justify-center" style={{ background: "var(--accent-light)" }}>
          {icon}
        </div>
        {trend && (
          <span style={{ color: trend === "up" ? "var(--success)" : trend === "down" ? "var(--danger)" : "var(--text-muted)" }}>
            {trend === "up" ? <ArrowUpRight size={14} /> : trend === "down" ? <ArrowDownRight size={14} /> : <Minus size={14} />}
          </span>
        )}
      </div>
      <div className="text-xl font-bold tabular-nums" style={{ color: "var(--text-primary)" }}>{value}</div>
      <div className="text-xs mt-0.5" style={{ color: "var(--text-muted)" }}>{label}</div>
      {sub && <div className="text-[10px] mt-1" style={{ color: "var(--text-muted)" }}>{sub}</div>}
    </div>
  );
}

function QuotaBar({ label, used, quota, color }: { label: string; used: number; quota: number; color: string }) {
  const pct = quota > 0 ? Math.min((used / quota) * 100, 100) : 0;
  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between">
        <span className="text-xs font-medium" style={{ color: "var(--text-secondary)" }}>{label}</span>
        <span className="text-[11px] tabular-nums" style={{ color: "var(--text-muted)" }}>
          {used} / {quota}
        </span>
      </div>
      <div className="h-2 rounded-full overflow-hidden" style={{ background: "var(--bg-tertiary)" }}>
        <div
          className="h-full rounded-full perf-bar"
          style={{ width: `${pct}%`, background: pct > 80 ? "var(--danger)" : pct > 50 ? "var(--warning)" : color }}
        />
      </div>
    </div>
  );
}

function EventTypeIcon({ type: t }: { type: string }) {
  const style = { width: 6, height: 6, borderRadius: "50%", flexShrink: 0 };
  const colors: Record<string, string> = {
    remote_chat: "var(--accent)",
    enrich: "var(--warning)",
    tool_call: "var(--success)",
    scheduled_task: "#a78bfa",
  };
  return <div style={{ ...style, background: colors[t] || "var(--text-muted)" }} />;
}

const EVENT_TYPE_LABELS: Record<string, string> = {
  remote_chat: "飞书对话",
  enrich: "数据预取",
  tool_call: "工具调用",
  scheduled_task: "定时任务",
};

export function PerformanceDashboard() {
  const [summary, setSummary] = useState<PerfSummary | null>(null);
  const [events, setEvents] = useState<PerfEvent[]>([]);
  const [searchStats, setSearchStats] = useState<SearchStats | null>(null);
  const [mcpServers, setMcpServers] = useState<McpServer[]>([]);
  const [loading, setLoading] = useState(true);
  const [lastRefresh, setLastRefresh] = useState<Date>(new Date());

  const fetchData = useCallback(async () => {
    setLoading(true);
    try {
      const [perfRes, searchRes, mcpRes] = await Promise.all([
        invoke<{ success: boolean; data?: any }>("get_perf_metrics"),
        invoke<{ success: boolean; data?: any }>("get_search_usage_stats"),
        invoke<{ success: boolean; data?: any }>("get_mcp_status"),
      ]);
      if (perfRes.success && perfRes.data) {
        setSummary(perfRes.data.summary);
        setEvents(perfRes.data.recent_events || []);
      }
      if (searchRes.success && searchRes.data) {
        setSearchStats(searchRes.data);
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
    const interval = setInterval(fetchData, 30000);
    return () => clearInterval(interval);
  }, [fetchData]);

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <header
        className="flex items-center justify-between px-5 h-12 border-b shrink-0"
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

      <div className="flex-1 overflow-y-auto p-5 space-y-5" style={{ background: "var(--bg-primary)" }}>
        {/* Stats grid */}
        <div className="grid grid-cols-4 gap-3">
          <StatCard
            icon={<Zap size={16} style={{ color: "var(--accent)" }} />}
            label="飞书对话"
            value={summary ? String(summary.chat_count) : "—"}
            sub={summary && summary.chat_avg_ms > 0 ? `平均 ${formatDuration(summary.chat_avg_ms)}` : undefined}
          />
          <StatCard
            icon={<Clock size={16} style={{ color: "var(--warning)" }} />}
            label="数据预取"
            value={summary ? String(summary.enrich_count) : "—"}
            sub={summary && summary.enrich_avg_ms > 0 ? `平均 ${formatDuration(summary.enrich_avg_ms)}` : undefined}
          />
          <StatCard
            icon={<Activity size={16} style={{ color: "var(--success)" }} />}
            label="工具调用"
            value={summary ? String(summary.tool_call_count) : "—"}
          />
          <StatCard
            icon={<Server size={16} style={{ color: "#a78bfa" }} />}
            label="定时任务"
            value={summary ? String(summary.scheduled_task_count) : "—"}
          />
        </div>

        <div className="grid grid-cols-2 gap-4">
          {/* Search API usage */}
          <div
            className="rounded-xl p-4 space-y-4"
            style={{ background: "var(--bg-secondary)", border: "1px solid var(--border)" }}
          >
            <div className="flex items-center justify-between">
              <h3 className="text-sm font-semibold" style={{ color: "var(--text-primary)" }}>搜索 API 用量</h3>
              <span className="text-[10px] px-1.5 py-0.5 rounded" style={{ background: "var(--bg-tertiary)", color: "var(--text-muted)" }}>
                本月
              </span>
            </div>
            {searchStats ? (
              <div className="space-y-3">
                <QuotaBar label="Tavily" used={searchStats.tavily.used} quota={searchStats.tavily.quota} color="var(--accent)" />
                <QuotaBar label="SerpApi" used={searchStats.serpapi.used} quota={searchStats.serpapi.quota} color="var(--success)" />
                <QuotaBar label="Brave" used={searchStats.brave.used} quota={searchStats.brave.quota} color="var(--warning)" />
              </div>
            ) : (
              <div className="text-xs" style={{ color: "var(--text-muted)" }}>加载中...</div>
            )}
          </div>

          {/* MCP Status */}
          <div
            className="rounded-xl p-4 space-y-3"
            style={{ background: "var(--bg-secondary)", border: "1px solid var(--border)" }}
          >
            <h3 className="text-sm font-semibold" style={{ color: "var(--text-primary)" }}>MCP 服务器</h3>
            {mcpServers.length > 0 ? (
              <div className="space-y-2">
                {mcpServers.map((srv, i) => (
                  <div key={i} className="flex items-center justify-between p-2.5 rounded-lg" style={{ background: "var(--bg-tertiary)" }}>
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
              <div className="flex flex-col items-center justify-center py-6 gap-2">
                <Server size={20} style={{ color: "var(--text-muted)", opacity: 0.4 }} />
                <span className="text-xs" style={{ color: "var(--text-muted)" }}>未连接 MCP 服务器</span>
              </div>
            )}
          </div>
        </div>

        {/* Recent events timeline */}
        <div
          className="rounded-xl p-4"
          style={{ background: "var(--bg-secondary)", border: "1px solid var(--border)" }}
        >
          <div className="flex items-center justify-between mb-3">
            <h3 className="text-sm font-semibold" style={{ color: "var(--text-primary)" }}>最近事件</h3>
            <span className="text-[10px] px-1.5 py-0.5 rounded" style={{ background: "var(--bg-tertiary)", color: "var(--text-muted)" }}>
              最近 50 条
            </span>
          </div>
          {events.length > 0 ? (
            <div className="space-y-1">
              <div className="flex items-center gap-3 px-2 py-1.5 text-[10px] font-medium" style={{ color: "var(--text-muted)" }}>
                <span className="w-16">时间</span>
                <span className="w-16">类型</span>
                <span className="flex-1">详情</span>
                <span className="w-16 text-right">耗时</span>
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
                    <span className="text-[11px] tabular-nums w-16 shrink-0" style={{ color: "var(--text-muted)" }}>
                      {ev.timestamp.split(" ")[1] || ev.timestamp}
                    </span>
                    <span className="flex items-center gap-1.5 w-16 shrink-0">
                      <EventTypeIcon type={ev.event_type} />
                      <span className="text-[11px]" style={{ color: "var(--text-secondary)" }}>
                        {EVENT_TYPE_LABELS[ev.event_type] || ev.event_type}
                      </span>
                    </span>
                    <span className="flex-1 text-[11px] truncate" style={{ color: "var(--text-primary)" }}>
                      {ev.label}
                    </span>
                    <span
                      className="text-[11px] tabular-nums w-16 text-right font-medium shrink-0"
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
              <span className="text-[10px]" style={{ color: "var(--text-muted)" }}>当有飞书对话或定时任务执行后，数据将自动出现</span>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
