import { create } from "zustand";

type View = "chat" | "settings" | "audit";

interface AppState {
  currentView: View;
  sidebarOpen: boolean;
  setView: (view: View) => void;
  toggleSidebar: () => void;
}

export const useAppStore = create<AppState>((set) => ({
  currentView: "chat",
  sidebarOpen: true,
  setView: (view) => set({ currentView: view }),
  toggleSidebar: () => set((s) => ({ sidebarOpen: !s.sidebarOpen })),
}));
