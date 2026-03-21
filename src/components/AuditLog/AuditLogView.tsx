import { useState } from "react";
import { Shield, Search, Filter, CheckCircle2, XCircle, Ban } from "lucide-react";

interface AuditEntry {
  id: string;
  timestamp: string;
  operation: string;
  risk_level: "safe" | "moderate" | "dangerous" | "forbidden";
  status: "approved" | "rejected" | "auto_approved" | "blocked";
  details: string;
  source: "local" | "feishu" | "wechat_work" | "scheduled";
}

const DEMO_ENTRIES: AuditEntry[] = [
  { id: "1", timestamp: "2026-03-15 10:32:05", operation: "read_file", risk_level: "safe", status: "auto_approved", details: "读取 src/main.tsx", source: "local" },
  { id: "2", timestamp: "2026-03-15 10:32:08", operation: "write_file", risk_level: "moderate", status: "approved", details: "写入 src/App.tsx", source: "local" },
  { id: "3", timestamp: "2026-03-15 10:33:12", operation: "execute_shell", risk_level: "dangerous", status: "approved", details: "git commit -m 'update'", source: "local" },
  { id: "4", timestamp: "2026-03-15 10:34:00", operation: "format_disk", risk_level: "forbidden", status: "blocked", details: "尝试格式化 D:", source: "local" },
  { id: "5", timestamp: "2026-03-15 10:35:20", operation: "list_directory", risk_level: "safe", status: "auto_approved", details: "列出 src/components/", source: "feishu" },
  { id: "6", timestamp: "2026-03-15 10:36:15", operation: "delete_file", risk_level: "dangerous", status: "rejected", details: "删除 package.json", source: "local" },
];

export function AuditLogView() {
  const [entries] = useState<AuditEntry[]>(DEMO_ENTRIES);
  const [filterLevel, setFilterLevel] = useState<string>("all");
  const [searchTerm, setSearchTerm] = useState("");

  const filtered = entries.filter((e) => {
    if (filterLevel !== "all" && e.risk_level !== filterLevel) return false;
    if (searchTerm && !e.operation.includes(searchTerm) && !e.details.includes(searchTerm)) return false;
    return true;
  });

  const statusIcon = (status: string) => {
    switch (status) {
      case "approved": return <CheckCircle2 size={13} style={{ color: "var(--success)" }} />;
      case "auto_approved": return <CheckCircle2 size={13} style={{ color: "var(--text-muted)" }} />;
      case "rejected": return <XCircle size={13} style={{ color: "var(--danger)" }} />;
      case "blocked": return <Ban size={13} style={{ color: "var(--danger)" }} />;
      default: return null;
    }
  };

  const statusLabel = (status: string) => {
    switch (status) {
      case "approved": return "已批准";
      case "auto_approved": return "自动通过";
      case "rejected": return "已拒绝";
      case "blocked": return "已阻止";
      default: return status;
    }
  };

  const riskBadge = (level: string) => {
    const styles: Record<string, { bg: string; label: string }> = {
      safe: { bg: "var(--success)", label: "安全" },
      moderate: { bg: "var(--warning)", label: "中风险" },
      dangerous: { bg: "var(--danger)", label: "高风险" },
      forbidden: { bg: "#1a1a1a", label: "禁止" },
    };
    const s = styles[level] || styles.safe;
    return (
      <span
        className="text-[10px] px-1.5 py-0.5 rounded text-white font-medium"
        style={{ background: s.bg }}
      >
        {s.label}
      </span>
    );
  };

  const sourceBadge = (source: string) => {
    const labels: Record<string, string> = {
      local: "本地",
      feishu: "飞书",
      wechat_work: "企业微信",
      scheduled: "定时",
    };
    return (
      <span
        className="text-[10px] px-1.5 py-0.5 rounded"
        style={{ background: "var(--bg-tertiary)", color: "var(--text-muted)" }}
      >
        {labels[source] || source}
      </span>
    );
  };

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div
        className="flex items-center justify-between px-5 h-12 border-b shrink-0"
        style={{ borderColor: "var(--border)", background: "var(--bg-secondary)" }}
      >
        <div className="flex items-center gap-2">
          <Shield size={16} style={{ color: "var(--accent)" }} />
          <h1 className="font-semibold text-sm">审计日志</h1>
          <span className="text-[11px] px-1.5 py-0.5 rounded" style={{ background: "var(--bg-tertiary)", color: "var(--text-muted)" }}>
            {filtered.length} 条记录
          </span>
        </div>
      </div>

      {/* Filters */}
      <div
        className="flex items-center gap-2 px-4 py-2 border-b shrink-0"
        style={{ borderColor: "var(--border)" }}
      >
        <div className="flex items-center gap-1.5 flex-1">
          <Search size={13} style={{ color: "var(--text-muted)" }} />
          <input
            type="text"
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            placeholder="搜索操作或详情..."
            className="flex-1 bg-transparent outline-none text-xs"
            style={{ color: "var(--text-primary)" }}
          />
        </div>
        <div className="flex items-center gap-1">
          <Filter size={12} style={{ color: "var(--text-muted)" }} />
          {["all", "safe", "moderate", "dangerous", "forbidden"].map((level) => (
            <button
              key={level}
              onClick={() => setFilterLevel(level)}
              className="text-[11px] px-2 py-0.5 rounded transition-colors"
              style={{
                background: filterLevel === level ? "var(--accent)" : "var(--bg-tertiary)",
                color: filterLevel === level ? "#fff" : "var(--text-muted)",
              }}
            >
              {level === "all" ? "全部" : level === "safe" ? "安全" : level === "moderate" ? "中" : level === "dangerous" ? "高" : "禁止"}
            </button>
          ))}
        </div>
      </div>

      {/* Table */}
      <div className="flex-1 overflow-y-auto">
        <table className="w-full text-xs">
          <thead>
            <tr style={{ background: "var(--bg-secondary)" }}>
              <th className="text-left px-4 py-2 font-medium" style={{ color: "var(--text-muted)" }}>时间</th>
              <th className="text-left px-4 py-2 font-medium" style={{ color: "var(--text-muted)" }}>操作</th>
              <th className="text-left px-4 py-2 font-medium" style={{ color: "var(--text-muted)" }}>风险</th>
              <th className="text-left px-4 py-2 font-medium" style={{ color: "var(--text-muted)" }}>结果</th>
              <th className="text-left px-4 py-2 font-medium" style={{ color: "var(--text-muted)" }}>来源</th>
              <th className="text-left px-4 py-2 font-medium" style={{ color: "var(--text-muted)" }}>详情</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((entry) => (
              <tr
                key={entry.id}
                className="border-b transition-colors"
                style={{ borderColor: "var(--border)" }}
              >
                <td className="px-4 py-2.5 whitespace-nowrap" style={{ color: "var(--text-muted)" }}>
                  {entry.timestamp.split(" ")[1]}
                </td>
                <td className="px-4 py-2.5 font-mono" style={{ color: "var(--text-primary)" }}>
                  {entry.operation}
                </td>
                <td className="px-4 py-2.5">{riskBadge(entry.risk_level)}</td>
                <td className="px-4 py-2.5">
                  <span className="flex items-center gap-1">
                    {statusIcon(entry.status)}
                    <span style={{ color: "var(--text-secondary)" }}>{statusLabel(entry.status)}</span>
                  </span>
                </td>
                <td className="px-4 py-2.5">{sourceBadge(entry.source)}</td>
                <td className="px-4 py-2.5 max-w-[200px] truncate" style={{ color: "var(--text-secondary)" }}>
                  {entry.details}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
