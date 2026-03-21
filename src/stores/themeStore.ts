import { create } from "zustand";

type Theme = "system" | "light" | "dark";

interface ThemeState {
  theme: Theme;
  setTheme: (theme: Theme) => void;
}

function applyTheme(theme: Theme) {
  const root = document.documentElement;
  root.removeAttribute("data-theme");

  if (theme === "system") {
    const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    root.setAttribute("data-theme", prefersDark ? "dark" : "light");
  } else {
    root.setAttribute("data-theme", theme);
  }

  localStorage.setItem("auto-crab-theme", theme);
}

const saved = (localStorage.getItem("auto-crab-theme") as Theme) || "system";
applyTheme(saved);

window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
  const current = localStorage.getItem("auto-crab-theme") as Theme;
  if (current === "system") applyTheme("system");
});

export const useThemeStore = create<ThemeState>((set) => ({
  theme: saved,
  setTheme: (theme) => {
    applyTheme(theme);
    set({ theme });
  },
}));
