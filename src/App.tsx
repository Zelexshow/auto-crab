import { Sidebar } from "./components/Sidebar/Sidebar";
import { ChatView } from "./components/Chat/ChatView";
import { SettingsView } from "./components/Settings/SettingsView";
import { useAppStore } from "./stores/appStore";
import { clsx } from "clsx";

function App() {
  const { currentView, sidebarOpen } = useAppStore();

  return (
    <div className="flex h-screen overflow-hidden" style={{ background: "var(--bg-primary)" }}>
      <Sidebar />
      <main
        className={clsx(
          "flex-1 flex flex-col transition-all duration-200 overflow-hidden",
          sidebarOpen ? "ml-0" : "ml-0",
        )}
      >
        {currentView === "chat" && <ChatView />}
        {currentView === "settings" && <SettingsView />}
        {currentView === "audit" && (
          <div className="flex-1 flex items-center justify-center" style={{ color: "var(--text-muted)" }}>
            <div className="text-center">
              <p className="text-lg font-medium">审计日志</p>
              <p className="text-sm mt-2">此功能正在开发中</p>
            </div>
          </div>
        )}
      </main>
    </div>
  );
}

export default App;
