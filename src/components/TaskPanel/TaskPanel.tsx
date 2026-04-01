import { useState, useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Activity, CheckCircle2, XCircle, Clock, ChevronDown, ChevronRight,
  FileText, Terminal, Globe, Loader2,
} from "lucide-react";
import { useChatStore } from "../../stores/chatStore";

interface TaskStep {
  id: string;
  type: "thinking" | "tool_call" | "tool_result" | "response" | "plan";
  tool?: string;
  content: string;
  status: "running" | "done" | "error" | "blocked";
  timestamp: number;
}

export function TaskPanel() {
  const [steps, setSteps] = useState<TaskStep[]>([]);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [isActive, setIsActive] = useState(false);
  const conversationId = useChatStore((s) => s.conversationId);
  const prevConvId = useRef(conversationId);

  useEffect(() => {
    if (prevConvId.current !== conversationId) {
      setSteps([]);
      setCollapsed(new Set());
      setIsActive(false);
      prevConvId.current = conversationId;
    }
  }, [conversationId]);

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
      <div className="h-full flex flex-col items-center justify-center gap-3 px-6">
        <div className="w-12 h-12 rounded-full flex items-center justify-center" style={{ background: "var(--accent-light)" }}>
          <Activity size={20} style={{ color: "var(--accent)", opacity: 0.6 }} />
        </div>
        <p className="text-[13px] text-center leading-5" style={{ color: "var(--text-muted)" }}>
          Agent 思考过程<br/>和工具调用将在这里显示
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
          <span className="text-[13px] font-medium">
            {isActive ? "执行中..." : `共 ${steps.length} 步`}
          </span>
        </div>
        <button
          onClick={() => setSteps([])}
          className="text-[12px] px-2 py-0.5 rounded transition-colors"
          style={{ color: "var(--text-muted)", background: "var(--bg-tertiary)" }}
        >
          清空
        </button>
      </div>

      <div className="flex-1 overflow-y-auto px-2.5 py-2 space-y-1.5">
        {steps.map((step) => {
          const isOpen = !collapsed.has(step.id);
          const borderColor = step.status === "error" ? "var(--danger)"
            : step.status === "blocked" ? "var(--warning)"
            : step.status === "running" ? "var(--accent)"
            : "var(--success)";
          return (
            <div key={step.id} className="animate-fade-in">
              <button
                onClick={() => toggleCollapse(step.id)}
                className="flex items-center gap-1.5 w-full text-left px-2.5 py-2 rounded-lg transition-all"
                style={{ background: "var(--bg-primary)", borderLeft: `3px solid ${borderColor}`, boxShadow: "var(--shadow-sm)" }}
              >
                {isOpen ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
                {getIcon(step)}
                <span className="text-[13px] truncate flex-1 font-medium" style={{ color: "var(--text-primary)" }}>
                  {step.type === "thinking" && (step.status === "running" ? "思考中..." : "思考完成")}
                  {step.type === "tool_call" && `${step.tool || "工具"}`}
                  {step.type === "tool_result" && `${step.tool || "工具"} ✓`}
                  {step.type === "plan" && "📋 任务计划"}
                  {step.type === "response" && "回复"}
                </span>
                <span className="text-[11px] shrink-0 tabular-nums" style={{ color: "var(--text-muted)" }}>
                  {new Date(step.timestamp).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit", second: "2-digit" })}
                </span>
              </button>
              {isOpen && step.content && (
                <pre
                  className="text-[12px] leading-relaxed px-3 py-2 ml-3 mt-1 rounded-lg overflow-x-auto whitespace-pre-wrap break-words"
                  style={{
                    background: "var(--bg-tertiary)",
                    color: "var(--text-secondary)",
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
