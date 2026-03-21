import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Activity, CheckCircle2, XCircle, Clock, ChevronDown, ChevronRight,
  FileText, Terminal, Globe, Loader2,
} from "lucide-react";

interface TaskStep {
  id: string;
  type: "thinking" | "tool_call" | "tool_result" | "response";
  tool?: string;
  content: string;
  status: "running" | "done" | "error" | "blocked";
  timestamp: number;
}

export function TaskPanel() {
  const [steps, setSteps] = useState<TaskStep[]>([]);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [isActive, setIsActive] = useState(false);

  useEffect(() => {
    const unlistenStep = listen<TaskStep>("agent-step", (event) => {
      setSteps((prev) => {
        const existing = prev.findIndex((s) => s.id === event.payload.id);
        if (existing >= 0) {
          const updated = [...prev];
          updated[existing] = event.payload;
          return updated;
        }
        return [...prev, event.payload];
      });
      setIsActive(event.payload.status === "running");
    });

    const unlistenDone = listen("agent-done", () => {
      setIsActive(false);
    });

    return () => {
      unlistenStep.then((fn) => fn());
      unlistenDone.then((fn) => fn());
    };
  }, []);

  const toggleCollapse = (id: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const getIcon = (step: TaskStep) => {
    if (step.status === "running") return <Loader2 size={13} className="animate-spin" style={{ color: "var(--accent)" }} />;
    if (step.status === "error") return <XCircle size={13} style={{ color: "var(--danger)" }} />;
    if (step.status === "blocked") return <Clock size={13} style={{ color: "var(--warning)" }} />;
    if (step.type === "tool_call") {
      if (step.tool?.includes("file") || step.tool?.includes("read") || step.tool?.includes("write"))
        return <FileText size={13} style={{ color: "var(--success)" }} />;
      if (step.tool?.includes("shell") || step.tool?.includes("execute"))
        return <Terminal size={13} style={{ color: "var(--warning)" }} />;
      if (step.tool?.includes("web") || step.tool?.includes("search"))
        return <Globe size={13} style={{ color: "var(--accent)" }} />;
    }
    return <CheckCircle2 size={13} style={{ color: "var(--success)" }} />;
  };

  if (steps.length === 0) {
    return (
      <div className="h-full flex flex-col items-center justify-center gap-2 px-4">
        <Activity size={32} style={{ color: "var(--text-muted)", opacity: 0.4 }} />
        <p className="text-xs text-center" style={{ color: "var(--text-muted)" }}>
          Agent 执行的思考过程和工具调用将在这里实时显示
        </p>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col">
      <div
        className="flex items-center justify-between px-3 py-2 border-b shrink-0"
        style={{ borderColor: "var(--border)" }}
      >
        <div className="flex items-center gap-1.5">
          {isActive ? (
            <Loader2 size={13} className="animate-spin" style={{ color: "var(--accent)" }} />
          ) : (
            <Activity size={13} style={{ color: "var(--text-muted)" }} />
          )}
          <span className="text-xs font-medium">
            {isActive ? "执行中..." : `共 ${steps.length} 步`}
          </span>
        </div>
        <button
          onClick={() => setSteps([])}
          className="text-[11px] px-2 py-0.5 rounded transition-colors"
          style={{ color: "var(--text-muted)", background: "var(--bg-tertiary)" }}
        >
          清空
        </button>
      </div>

      <div className="flex-1 overflow-y-auto px-2 py-2 space-y-1">
        {steps.map((step) => {
          const isOpen = !collapsed.has(step.id);
          return (
            <div key={step.id}>
              <button
                onClick={() => toggleCollapse(step.id)}
                className="flex items-center gap-1.5 w-full text-left px-2 py-1.5 rounded transition-colors"
                style={{ background: "var(--bg-secondary)" }}
              >
                {isOpen ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
                {getIcon(step)}
                <span className="text-[12px] truncate flex-1" style={{ color: "var(--text-primary)" }}>
                  {step.type === "thinking" && "思考中..."}
                  {step.type === "tool_call" && `调用 ${step.tool || "工具"}`}
                  {step.type === "tool_result" && `${step.tool || "工具"} 返回结果`}
                  {step.type === "response" && "生成回复"}
                </span>
                <span className="text-[10px] shrink-0" style={{ color: "var(--text-muted)" }}>
                  {new Date(step.timestamp).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit", second: "2-digit" })}
                </span>
              </button>
              {isOpen && step.content && (
                <pre
                  className="text-[11px] leading-4 px-3 py-2 mx-2 mt-0.5 rounded overflow-x-auto whitespace-pre-wrap break-words"
                  style={{
                    background: "var(--bg-primary)",
                    color: "var(--text-secondary)",
                    border: "1px solid var(--border)",
                    maxHeight: 200,
                  }}
                >
                  {step.content.length > 2000 ? step.content.slice(0, 2000) + "\n..." : step.content}
                </pre>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
