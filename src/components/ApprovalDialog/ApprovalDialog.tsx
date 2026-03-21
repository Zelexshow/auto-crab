import { useState, useEffect } from "react";
import { ShieldAlert, Check, X } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

interface PendingApproval {
  id: string;
  operation: string;
  risk_level: string;
  description: string;
}

export function ApprovalDialog() {
  const [approvals, setApprovals] = useState<PendingApproval[]>([]);

  useEffect(() => {
    const unlisten = listen<PendingApproval>("approval-request", (event) => {
      setApprovals((prev) => [...prev, event.payload]);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  const handleApprove = async (id: string) => {
    await invoke("approve_operation", { id });
    setApprovals((prev) => prev.filter((a) => a.id !== id));
  };

  const handleReject = async (id: string) => {
    await invoke("reject_operation", { id, reason: "用户拒绝" });
    setApprovals((prev) => prev.filter((a) => a.id !== id));
  };

  if (approvals.length === 0) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center" style={{ background: "rgba(0,0,0,0.4)" }}>
      <div
        className="w-96 rounded-xl shadow-2xl overflow-hidden"
        style={{ background: "var(--bg-primary)", border: "1px solid var(--border)" }}
      >
        {approvals.map((approval) => (
          <div key={approval.id} className="p-5">
            <div className="flex items-center gap-2 mb-3">
              <ShieldAlert size={20} style={{ color: "var(--warning)" }} />
              <h3 className="font-semibold text-sm">操作审批</h3>
            </div>

            <div
              className="rounded-lg p-3 mb-4 text-sm"
              style={{ background: "var(--bg-secondary)" }}
            >
              <p className="font-medium mb-1">{approval.operation}</p>
              <p className="text-xs" style={{ color: "var(--text-muted)" }}>
                {approval.description}
              </p>
              <div className="mt-2">
                <span
                  className="text-[11px] px-2 py-0.5 rounded"
                  style={{
                    background: approval.risk_level === "dangerous" ? "var(--danger)" : "var(--warning)",
                    color: "#fff",
                  }}
                >
                  {approval.risk_level === "dangerous" ? "高风险" : "中风险"}
                </span>
              </div>
            </div>

            <div className="flex gap-2">
              <button
                onClick={() => handleReject(approval.id)}
                className="flex-1 flex items-center justify-center gap-1.5 px-4 py-2 rounded-lg text-sm transition-colors"
                style={{ background: "var(--bg-tertiary)", color: "var(--text-secondary)" }}
              >
                <X size={14} /> 拒绝
              </button>
              <button
                onClick={() => handleApprove(approval.id)}
                className="flex-1 flex items-center justify-center gap-1.5 px-4 py-2 rounded-lg text-sm text-white transition-colors"
                style={{ background: "#07c160" }}
              >
                <Check size={14} /> 允许执行
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
