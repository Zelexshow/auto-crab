import { useState, useEffect } from "react";
import { Sidebar } from "./components/Sidebar/Sidebar";
import { ChatView } from "./components/Chat/ChatView";
import { SettingsView } from "./components/Settings/SettingsView";
import { AuditLogView } from "./components/AuditLog/AuditLogView";
import { ApprovalDialog } from "./components/ApprovalDialog/ApprovalDialog";
import { OnboardingWizard } from "./components/Onboarding/OnboardingWizard";
import { TaskPanel } from "./components/TaskPanel/TaskPanel";
import { useAppStore } from "./stores/appStore";

function App() {
  const { currentView } = useAppStore();
  const [showOnboarding, setShowOnboarding] = useState(false);

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

  return (
    <div className="flex h-screen overflow-hidden" style={{ background: "var(--bg-primary)" }}>
      {showOnboarding && <OnboardingWizard onComplete={handleOnboardingComplete} />}
      <ApprovalDialog />
      <Sidebar />
      <main className="flex-1 flex overflow-hidden">
        <div className="flex-1 flex flex-col overflow-hidden">
          {currentView === "chat" && <ChatView />}
          {currentView === "settings" && <SettingsView />}
          {currentView === "audit" && <AuditLogView />}
        </div>
        {currentView === "chat" && (
          <div className="w-72 border-l shrink-0 overflow-y-auto" style={{ borderColor: "var(--border)", background: "var(--bg-secondary)" }}>
            <TaskPanel />
          </div>
        )}
      </main>
    </div>
  );
}

export default App;
