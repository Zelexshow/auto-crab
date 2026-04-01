import { create } from "zustand";

type View = "chat" | "settings" | "audit" | "dashboard";

const SIDEBAR_MIN = 180;
const SIDEBAR_MAX = 360;
const SIDEBAR_COLLAPSED = 56;

const TOOL_PANEL_MIN = 200;
const TOOL_PANEL_MAX = 520;
const TOOL_PANEL_DEFAULT = 300;

interface AppState {
  currentView: View;
  sidebarWidth: number;
  sidebarCollapsed: boolean;
  toolPanelWidth: number;
  setView: (view: View) => void;
  setSidebarWidth: (width: number) => void;
  setToolPanelWidth: (width: number) => void;
  toggleSidebar: () => void;
  sidebarOpen: boolean;
}

const savedWidth = parseInt(localStorage.getItem("auto-crab-sidebar-width") || "240");
const savedCollapsed = localStorage.getItem("auto-crab-sidebar-collapsed") === "true";
const savedToolWidth = parseInt(localStorage.getItem("auto-crab-toolpanel-width") || String(TOOL_PANEL_DEFAULT));

export const useAppStore = create<AppState>((set) => ({
  currentView: "chat",
  sidebarWidth: Math.max(SIDEBAR_MIN, Math.min(SIDEBAR_MAX, savedWidth)),
  sidebarCollapsed: savedCollapsed,
  sidebarOpen: !savedCollapsed,
  toolPanelWidth: Math.max(TOOL_PANEL_MIN, Math.min(TOOL_PANEL_MAX, savedToolWidth)),
  setView: (view) => set({ currentView: view }),
  setSidebarWidth: (width) => {
    const clamped = Math.max(SIDEBAR_MIN, Math.min(SIDEBAR_MAX, width));
    localStorage.setItem("auto-crab-sidebar-width", String(clamped));
    set({ sidebarWidth: clamped });
  },
  setToolPanelWidth: (width) => {
    const clamped = Math.max(TOOL_PANEL_MIN, Math.min(TOOL_PANEL_MAX, width));
    localStorage.setItem("auto-crab-toolpanel-width", String(clamped));
    set({ toolPanelWidth: clamped });
  },
  toggleSidebar: () =>
    set((s) => {
      const collapsed = !s.sidebarCollapsed;
      localStorage.setItem("auto-crab-sidebar-collapsed", String(collapsed));
      return { sidebarCollapsed: collapsed, sidebarOpen: !collapsed };
    }),
}));

export { SIDEBAR_MIN, SIDEBAR_MAX, SIDEBAR_COLLAPSED, TOOL_PANEL_MIN, TOOL_PANEL_MAX };
