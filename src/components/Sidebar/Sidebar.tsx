import { MessageSquare, Settings, Shield, PanelLeftClose, PanelLeftOpen, Plus } from "lucide-react";
import { useAppStore } from "../../stores/appStore";
import { useChatStore } from "../../stores/chatStore";
import { clsx } from "clsx";

export function Sidebar() {
  const { currentView, setView, sidebarOpen, toggleSidebar } = useAppStore();
  const clearMessages = useChatStore((s) => s.clearMessages);

  const navItems = [
    { id: "chat" as const, label: "对话", icon: MessageSquare },
    { id: "settings" as const, label: "设置", icon: Settings },
    { id: "audit" as const, label: "审计", icon: Shield },
  ];

  return (
    <aside
      className={clsx(
        "flex flex-col border-r transition-all duration-200 shrink-0",
        sidebarOpen ? "w-56" : "w-14",
      )}
      style={{
        background: "var(--bg-secondary)",
        borderColor: "var(--border)",
      }}
    >
      {/* Logo area */}
      <div
        className="flex items-center gap-2 px-3 h-14 border-b shrink-0"
        style={{ borderColor: "var(--border)" }}
      >
        <div
          className="w-8 h-8 rounded-lg flex items-center justify-center text-white font-bold text-sm shrink-0"
          style={{ background: "var(--accent)" }}
        >
          🦀
        </div>
        {sidebarOpen && (
          <span className="font-semibold text-sm whitespace-nowrap">Auto Crab</span>
        )}
      </div>

      {/* New chat */}
      <div className="px-2 pt-3 pb-1">
        <button
          onClick={() => {
            clearMessages();
            setView("chat");
          }}
          className={clsx(
            "flex items-center gap-2 w-full rounded-lg px-3 py-2 text-sm font-medium transition-colors",
            "hover:opacity-80",
          )}
          style={{
            background: "var(--accent)",
            color: "#fff",
          }}
        >
          <Plus size={16} />
          {sidebarOpen && "新对话"}
        </button>
      </div>

      {/* Nav */}
      <nav className="flex-1 px-2 py-2 space-y-1">
        {navItems.map((item) => {
          const Icon = item.icon;
          const active = currentView === item.id;
          return (
            <button
              key={item.id}
              onClick={() => setView(item.id)}
              className={clsx(
                "flex items-center gap-2 w-full rounded-lg px-3 py-2 text-sm transition-colors",
              )}
              style={{
                background: active ? "var(--bg-tertiary)" : "transparent",
                color: active ? "var(--text-primary)" : "var(--text-secondary)",
              }}
            >
              <Icon size={18} />
              {sidebarOpen && item.label}
            </button>
          );
        })}
      </nav>

      {/* Collapse toggle */}
      <div className="px-2 pb-3">
        <button
          onClick={toggleSidebar}
          className="flex items-center gap-2 w-full rounded-lg px-3 py-2 text-sm transition-colors"
          style={{ color: "var(--text-muted)" }}
        >
          {sidebarOpen ? <PanelLeftClose size={18} /> : <PanelLeftOpen size={18} />}
          {sidebarOpen && "收起侧栏"}
        </button>
      </div>
    </aside>
  );
}
