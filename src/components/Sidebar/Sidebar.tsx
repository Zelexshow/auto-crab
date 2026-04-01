import { useEffect, useState, useRef, useCallback } from "react";
import {
  MessageSquare, Settings, Shield, PanelLeftClose, PanelLeftOpen,
  Plus, Trash2, Pencil, Check, X, BarChart3, GripVertical,
} from "lucide-react";
import { useAppStore, SIDEBAR_MIN, SIDEBAR_MAX, SIDEBAR_COLLAPSED } from "../../stores/appStore";
import { useChatStore } from "../../stores/chatStore";

export function Sidebar() {
  const {
    currentView, setView,
    sidebarWidth, setSidebarWidth,
    sidebarCollapsed, toggleSidebar,
  } = useAppStore();
  const {
    conversations, conversationId, newConversation,
    loadConversation, deleteConversation, refreshConversationList,
    renameConversation,
  } = useChatStore();

  const [editingId, setEditingId] = useState<string | null>(null);
  const [editTitle, setEditTitle] = useState("");
  const [isResizing, setIsResizing] = useState(false);
  const sidebarRef = useRef<HTMLElement>(null);
  const startXRef = useRef(0);
  const startWidthRef = useRef(0);

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

  const onResizeStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setIsResizing(true);
    startXRef.current = e.clientX;
    startWidthRef.current = sidebarWidth;
  }, [sidebarWidth]);

  useEffect(() => {
    if (!isResizing) return;

    const onMouseMove = (e: MouseEvent) => {
      const delta = e.clientX - startXRef.current;
      setSidebarWidth(startWidthRef.current + delta);
    };

    const onMouseUp = () => {
      setIsResizing(false);
    };

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
  }, [isResizing, setSidebarWidth]);

  const handleResizeDoubleClick = useCallback(() => {
    toggleSidebar();
  }, [toggleSidebar]);

  const expanded = !sidebarCollapsed;
  const actualWidth = sidebarCollapsed ? SIDEBAR_COLLAPSED : sidebarWidth;

  const navItems = [
    { id: "chat" as const, label: "对话", icon: MessageSquare },
    { id: "dashboard" as const, label: "监控", icon: BarChart3 },
    { id: "settings" as const, label: "设置", icon: Settings },
    { id: "audit" as const, label: "审计", icon: Shield },
  ];

  return (
    <aside
      ref={sidebarRef}
      className="relative flex flex-col shrink-0 select-none"
      style={{
        width: actualWidth,
        minWidth: sidebarCollapsed ? SIDEBAR_COLLAPSED : SIDEBAR_MIN,
        maxWidth: sidebarCollapsed ? SIDEBAR_COLLAPSED : SIDEBAR_MAX,
        background: "var(--bg-secondary)",
        transition: isResizing ? "none" : "width 0.2s ease",
        zIndex: 10,
      }}
    >
      {/* Logo */}
      <div
        className="flex items-center gap-3 shrink-0"
        style={{ padding: expanded ? "18px 18px 14px" : "18px 0 14px", justifyContent: expanded ? "flex-start" : "center" }}
      >
        <div
          className="w-10 h-10 rounded-xl flex items-center justify-center text-lg shrink-0"
          style={{
            background: "linear-gradient(135deg, var(--accent), var(--accent-hover))",
            boxShadow: "0 4px 12px rgba(99, 102, 241, 0.3)",
          }}
        >
          🦀
        </div>
        {expanded && (
          <div className="flex flex-col min-w-0">
            <span className="font-bold text-[15px] tracking-tight truncate">Auto Crab</span>
            <span className="text-[11px]" style={{ color: "var(--text-muted)" }}>AI 桌面助理</span>
          </div>
        )}
      </div>

      {/* New chat */}
      <div style={{ padding: expanded ? "0 14px 8px" : "0 8px 8px" }}>
        <button
          onClick={() => {
            newConversation();
            setView("chat");
          }}
          className="flex items-center justify-center gap-2 w-full rounded-xl text-[13px] font-medium"
          style={{
            background: "var(--accent)",
            color: "#fff",
            boxShadow: "0 2px 8px rgba(99, 102, 241, 0.25)",
            padding: expanded ? "10px 16px" : "10px 0",
          }}
        >
          <Plus size={16} strokeWidth={2.5} />
          {expanded && "新对话"}
        </button>
      </div>

      {/* Separator */}
      <div style={{ padding: "0 14px", margin: "4px 0 6px" }}>
        <div style={{ height: 1, background: "var(--border)", opacity: 0.6 }} />
      </div>

      {/* Nav */}
      <nav style={{ padding: expanded ? "0 10px" : "0 6px" }} className="space-y-0.5">
        {navItems.map((item) => {
          const Icon = item.icon;
          const active = currentView === item.id;
          return (
            <button
              key={item.id}
              onClick={() => setView(item.id)}
              className="flex items-center w-full rounded-lg text-[13px] font-medium transition-all"
              style={{
                background: active ? "var(--accent-light)" : "transparent",
                color: active ? "var(--accent)" : "var(--text-secondary)",
                padding: expanded ? "9px 12px" : "9px 0",
                gap: expanded ? 10 : 0,
                justifyContent: expanded ? "flex-start" : "center",
              }}
              title={expanded ? undefined : item.label}
            >
              <Icon size={18} strokeWidth={active ? 2.2 : 1.6} />
              {expanded && item.label}
            </button>
          );
        })}
      </nav>

      {/* Conversation history */}
      {expanded && conversations.length > 0 && (
        <div className="flex-1 overflow-y-auto mt-4" style={{ padding: "0 10px" }}>
          <div style={{ padding: "0 4px", margin: "0 0 8px" }}>
            <div style={{ height: 1, background: "var(--border)", opacity: 0.6 }} />
          </div>
          <p className="text-[10px] font-semibold px-2 pb-2 uppercase tracking-widest" style={{ color: "var(--text-muted)" }}>
            历史对话
          </p>
          <div className="space-y-0.5">
            {conversations.slice(0, 30).map((conv) => {
              const active = conv.id === conversationId;
              const isEditing = editingId === conv.id;

              return (
                <div
                  key={conv.id}
                  className="group flex items-center rounded-lg transition-all"
                  style={{
                    background: active ? "var(--bg-tertiary)" : "transparent",
                  }}
                >
                  {isEditing ? (
                    <div className="flex items-center gap-1.5 flex-1 px-2.5 py-2">
                      <input
                        type="text"
                        value={editTitle}
                        onChange={(e) => setEditTitle(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") confirmRename();
                          if (e.key === "Escape") cancelRename();
                        }}
                        className="flex-1 text-[12px] rounded-md px-2 py-1 outline-none min-w-0"
                        style={{
                          background: "var(--bg-primary)",
                          border: "1px solid var(--accent)",
                          color: "var(--text-primary)",
                        }}
                        autoFocus
                      />
                      <button onClick={confirmRename} className="p-1" style={{ color: "var(--success)" }}>
                        <Check size={13} />
                      </button>
                      <button onClick={cancelRename} className="p-1" style={{ color: "var(--text-muted)" }}>
                        <X size={13} />
                      </button>
                    </div>
                  ) : (
                    <>
                      <button
                        onClick={() => {
                          loadConversation(conv.id);
                          setView("chat");
                        }}
                        className="flex-1 text-left min-w-0"
                        style={{ padding: "8px 10px" }}
                      >
                        <p
                          className="text-[12px] truncate leading-snug"
                          style={{ color: active ? "var(--text-primary)" : "var(--text-secondary)" }}
                        >
                          {conv.title}
                        </p>
                        <p className="text-[10px] mt-0.5" style={{ color: "var(--text-muted)" }}>
                          {conv.message_count} 条消息
                        </p>
                      </button>
                      <div className="shrink-0 flex items-center gap-0.5 mr-1.5 opacity-0 group-hover:opacity-70 transition-opacity">
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            startRename(conv.id, conv.title);
                          }}
                          className="p-1 rounded-md"
                          style={{ color: "var(--text-muted)" }}
                          title="重命名"
                        >
                          <Pencil size={12} />
                        </button>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            deleteConversation(conv.id);
                          }}
                          className="p-1 rounded-md"
                          style={{ color: "var(--text-muted)" }}
                          title="删除"
                        >
                          <Trash2 size={12} />
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
      <div className="mt-auto" style={{ padding: expanded ? "8px 14px 12px" : "8px 6px 12px" }}>
        <button
          onClick={toggleSidebar}
          className="flex items-center w-full rounded-lg text-xs transition-colors"
          style={{
            color: "var(--text-muted)",
            padding: expanded ? "7px 10px" : "7px 0",
            gap: expanded ? 8 : 0,
            justifyContent: expanded ? "flex-start" : "center",
          }}
          title={expanded ? "收起侧边栏" : "展开侧边栏"}
        >
          {expanded ? <PanelLeftClose size={15} /> : <PanelLeftOpen size={15} />}
          {expanded && "收起"}
        </button>
      </div>

      {/* Resize handle */}
      {!sidebarCollapsed && (
        <div
          className="sidebar-resize-handle"
          onMouseDown={onResizeStart}
          onDoubleClick={handleResizeDoubleClick}
          title="拖动调整宽度，双击折叠"
        >
          <div className="sidebar-resize-indicator">
            <GripVertical size={10} />
          </div>
        </div>
      )}
    </aside>
  );
}
