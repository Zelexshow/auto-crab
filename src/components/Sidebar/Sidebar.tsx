import { useEffect, useState } from "react";
import {
  MessageSquare, Settings, Shield, PanelLeftClose, PanelLeftOpen,
  Plus, Trash2, Pencil, Check, X,
} from "lucide-react";
import { useAppStore } from "../../stores/appStore";
import { useChatStore } from "../../stores/chatStore";
import { clsx } from "clsx";

export function Sidebar() {
  const { currentView, setView, sidebarOpen, toggleSidebar } = useAppStore();
  const {
    conversations, conversationId, newConversation,
    loadConversation, deleteConversation, refreshConversationList,
    renameConversation,
  } = useChatStore();

  const [editingId, setEditingId] = useState<string | null>(null);
  const [editTitle, setEditTitle] = useState("");

  useEffect(() => {
    refreshConversationList();
  }, []);

  const startRename = (id: string, currentTitle: string) => {
    setEditingId(id);
    setEditTitle(currentTitle);
  };

  const confirmRename = () => {
    if (editingId && editTitle.trim()) {
      renameConversation(editingId, editTitle.trim());
    }
    setEditingId(null);
  };

  const cancelRename = () => {
    setEditingId(null);
  };

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
      {/* Logo */}
      <div
        className="flex items-center gap-2 px-3 h-12 border-b shrink-0"
        style={{ borderColor: "var(--border)" }}
      >
        <div
          className="w-7 h-7 rounded-md flex items-center justify-center text-white font-bold text-xs shrink-0"
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
            newConversation();
            setView("chat");
          }}
          className="flex items-center gap-2 w-full rounded-md px-3 py-1.5 text-xs font-medium transition-colors"
          style={{ background: "#07c160", color: "#fff" }}
        >
          <Plus size={14} />
          {sidebarOpen && "新对话"}
        </button>
      </div>

      {/* Nav */}
      <nav className="px-2 py-1 space-y-0.5">
        {navItems.map((item) => {
          const Icon = item.icon;
          const active = currentView === item.id;
          return (
            <button
              key={item.id}
              onClick={() => setView(item.id)}
              className="flex items-center gap-2 w-full rounded-md px-3 py-1.5 text-xs transition-colors"
              style={{
                background: active ? "var(--bg-tertiary)" : "transparent",
                color: active ? "var(--text-primary)" : "var(--text-secondary)",
              }}
            >
              <Icon size={15} />
              {sidebarOpen && item.label}
            </button>
          );
        })}
      </nav>

      {/* Conversation history */}
      {sidebarOpen && conversations.length > 0 && (
        <div className="flex-1 overflow-y-auto px-2 pt-2 border-t mt-1" style={{ borderColor: "var(--border)" }}>
          <p className="text-[10px] font-medium px-3 py-1 uppercase tracking-wider" style={{ color: "var(--text-muted)" }}>
            历史对话
          </p>
          <div className="space-y-0.5">
            {conversations.slice(0, 30).map((conv) => {
              const active = conv.id === conversationId;
              const isEditing = editingId === conv.id;

              return (
                <div
                  key={conv.id}
                  className="group flex items-center rounded-md transition-colors"
                  style={{
                    background: active ? "var(--bg-tertiary)" : "transparent",
                  }}
                >
                  {isEditing ? (
                    <div className="flex items-center gap-1 flex-1 px-2 py-1">
                      <input
                        type="text"
                        value={editTitle}
                        onChange={(e) => setEditTitle(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") confirmRename();
                          if (e.key === "Escape") cancelRename();
                        }}
                        className="flex-1 text-xs rounded px-1.5 py-0.5 outline-none min-w-0"
                        style={{
                          background: "var(--bg-primary)",
                          border: "1px solid var(--accent)",
                          color: "var(--text-primary)",
                        }}
                        autoFocus
                      />
                      <button onClick={confirmRename} className="p-0.5" style={{ color: "var(--success)" }}>
                        <Check size={12} />
                      </button>
                      <button onClick={cancelRename} className="p-0.5" style={{ color: "var(--text-muted)" }}>
                        <X size={12} />
                      </button>
                    </div>
                  ) : (
                    <>
                      <button
                        onClick={() => {
                          loadConversation(conv.id);
                          setView("chat");
                        }}
                        className="flex-1 text-left px-3 py-1.5 min-w-0"
                      >
                        <p
                          className="text-xs truncate"
                          style={{ color: active ? "var(--text-primary)" : "var(--text-secondary)" }}
                        >
                          {conv.title}
                        </p>
                        <p className="text-[10px]" style={{ color: "var(--text-muted)" }}>
                          {conv.message_count} 条消息
                        </p>
                      </button>
                      <div className="shrink-0 flex items-center gap-0.5 mr-1 opacity-0 group-hover:opacity-60 transition-opacity">
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            startRename(conv.id, conv.title);
                          }}
                          className="p-1 rounded hover:opacity-100"
                          style={{ color: "var(--text-muted)" }}
                          title="重命名"
                        >
                          <Pencil size={11} />
                        </button>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            deleteConversation(conv.id);
                          }}
                          className="p-1 rounded hover:opacity-100"
                          style={{ color: "var(--text-muted)" }}
                          title="删除"
                        >
                          <Trash2 size={11} />
                        </button>
                      </div>
                    </>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* Collapse toggle */}
      <div className="px-2 pb-3 mt-auto">
        <button
          onClick={toggleSidebar}
          className="flex items-center gap-2 w-full rounded-md px-3 py-1.5 text-xs transition-colors"
          style={{ color: "var(--text-muted)" }}
        >
          {sidebarOpen ? <PanelLeftClose size={15} /> : <PanelLeftOpen size={15} />}
          {sidebarOpen && "收起"}
        </button>
      </div>
    </aside>
  );
}
