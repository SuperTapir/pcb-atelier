import {
  createContext,
  type ReactNode,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useState,
} from "react";

import {
  normalizeThemePreference,
  resolveThemePreference,
  THEME_STORAGE_KEY,
  themeColorFor,
  type ResolvedTheme,
  type ThemePreference,
} from "@/features/theme/theme-state";

interface ThemeContextValue {
  preference: ThemePreference;
  resolvedTheme: ResolvedTheme;
  setPreference: (preference: ThemePreference) => void;
}

const ThemeContext = createContext<ThemeContextValue>({
  preference: "system",
  resolvedTheme: "light",
  setPreference: () => undefined,
});

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [preference, setPreferenceState] = useState<ThemePreference>(() =>
    normalizeThemePreference(globalThis.localStorage?.getItem(THEME_STORAGE_KEY)),
  );
  const [systemDark, setSystemDark] = useState(() =>
    globalThis.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false,
  );
  const resolvedTheme = resolveThemePreference(preference, systemDark);

  useEffect(() => {
    const media = globalThis.matchMedia?.("(prefers-color-scheme: dark)");
    if (!media) return;
    const handleChange = (event: MediaQueryListEvent) =>
      setSystemDark(event.matches);
    media.addEventListener("change", handleChange);
    return () => media.removeEventListener("change", handleChange);
  }, []);

  useLayoutEffect(() => {
    const root = globalThis.document?.documentElement;
    if (!root) return;
    root.dataset.theme = resolvedTheme;
    root.style.colorScheme = resolvedTheme;
    globalThis.document
      .querySelector<HTMLMetaElement>('meta[name="theme-color"]')
      ?.setAttribute("content", themeColorFor(resolvedTheme));
  }, [resolvedTheme]);

  const value = useMemo<ThemeContextValue>(
    () => ({
      preference,
      resolvedTheme,
      setPreference: (nextPreference) => {
        globalThis.localStorage?.setItem(THEME_STORAGE_KEY, nextPreference);
        setPreferenceState(nextPreference);
      },
    }),
    [preference, resolvedTheme],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme() {
  return useContext(ThemeContext);
}
