import { useState, useEffect, useCallback, useRef } from "react";
import { GripVertical } from "lucide-react";
import { Sidebar } from "./components/Sidebar/Sidebar";
import { ChatView } from "./components/Chat/ChatView";
import { SettingsView } from "./components/Settings/SettingsView";
import { AuditLogView } from "./components/AuditLog/AuditLogView";
import { PerformanceDashboard } from "./components/Dashboard/PerformanceDashboard";
import { ApprovalDialog } from "./components/ApprovalDialog/ApprovalDialog";
import { OnboardingWizard } from "./components/Onboarding/OnboardingWizard";
import { TaskPanel } from "./components/TaskPanel/TaskPanel";
import { useAppStore } from "./stores/appStore";

function App() {
  const { currentView, toolPanelWidth, setToolPanelWidth } = useAppStore();
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [isResizingTool, setIsResizingTool] = useState(false);
  const toolStartXRef = useRef(0);
  const toolStartWidthRef = useRef(0);

  useEffect(() => {
    const hasCompleted = localStorage.getItem("auto-crab-onboarded");
    if (!hasCompleted) {
      setShowOnboarding(true);
    }
  }, []);

  const handleOnboardingComplete = () => {
    localStorage.setItem("auto-crab-onboarded", "true");
    setShowOnboarding(false);
  };

  const onToolResizeStart = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      setIsResizingTool(true);
      toolStartXRef.current = e.clientX;
      toolStartWidthRef.current = toolPanelWidth;
    },
    [toolPanelWidth],
  );

  useEffect(() => {
    if (!isResizingTool) return;

    const onMouseMove = (e: MouseEvent) => {
      const delta = toolStartXRef.current - e.clientX;
      setToolPanelWidth(toolStartWidthRef.current + delta);
    };
    const onMouseUp = () => setIsResizingTool(false);

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";

    return () => {
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
  }, [isResizingTool, setToolPanelWidth]);

  return (
    <div className="flex h-screen overflow-hidden" style={{ background: "var(--bg-primary)" }}>
      {showOnboarding && <OnboardingWizard onComplete={handleOnboardingComplete} />}
      <ApprovalDialog />
      <Sidebar />
      <main className="flex-1 flex overflow-hidden" style={{ position: "relative" }}>
        <div className="flex-1 flex flex-col overflow-hidden">
          {currentView === "chat" && <ChatView />}
          {currentView === "dashboard" && <PerformanceDashboard />}
          {currentView === "settings" && <SettingsView />}
          {currentView === "audit" && <AuditLogView />}
        </div>
        {currentView === "chat" && (
          <div
            className="relative shrink-0 overflow-y-auto"
            style={{
              width: toolPanelWidth,
              background: "var(--bg-secondary)",
              transition: isResizingTool ? "none" : "width 0.15s ease",
            }}
          >
            {/* Resize handle (left edge) */}
            <div
              className="toolpanel-resize-handle"
              onMouseDown={onToolResizeStart}
            >
              <div className="sidebar-resize-indicator">
                <GripVertical size={10} />
              </div>
            </div>
            <div className="px-3 py-2.5 flex items-center gap-2" style={{ borderBottom: "1px solid var(--border)" }}>
              <div className="w-2 h-2 rounded-full animate-pulse-dot" style={{ background: "var(--accent)" }} />
              <span className="text-[13px] font-semibold tracking-wide" style={{ color: "var(--text-secondary)" }}>工具执行面板</span>
            </div>
            <TaskPanel />
          </div>
        )}
      </main>
    </div>
  );
}

export default App;
