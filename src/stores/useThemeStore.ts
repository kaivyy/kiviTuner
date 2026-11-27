import { create } from 'zustand';

export type KiviTheme = 'dark' | 'light';

const THEME_STORAGE_KEY = 'kivituner-theme';

function detectInitialTheme(): KiviTheme {
  if (typeof window !== 'undefined') {
    try {
      const saved = window.localStorage?.getItem(THEME_STORAGE_KEY);
      if (saved === 'light' || saved === 'dark') {
        return saved;
      }
    } catch {
      // Ignore storage access restrictions.
    }
  }
  return 'dark';
}

export function applyThemeToDocument(theme: KiviTheme): void {
  if (typeof document !== 'undefined') {
    const root = document.documentElement;
    root.dataset.theme = theme;
    root.classList.toggle('dark', theme === 'dark');
  }
  if (typeof window !== 'undefined') {
    try {
      window.localStorage?.setItem(THEME_STORAGE_KEY, theme);
    } catch {
      // Ignore storage access restrictions.
    }
  }
}

export interface ThemeStoreState {
  theme: KiviTheme;
  setTheme: (theme: KiviTheme) => void;
  toggleTheme: () => KiviTheme;
}

export const useThemeStore = create<ThemeStoreState>((set, get) => ({
  theme: detectInitialTheme(),
  setTheme: (theme) => {
    applyThemeToDocument(theme);
    set({ theme });
  },
  toggleTheme: () => {
    const next: KiviTheme = get().theme === 'dark' ? 'light' : 'dark';
    applyThemeToDocument(next);
    set({ theme: next });
    return next;
  },
}));
