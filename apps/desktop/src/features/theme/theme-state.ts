export type ThemePreference = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

export const THEME_STORAGE_KEY = "pcb-atelier.theme";

export function normalizeThemePreference(
  value: string | null,
): ThemePreference {
  return value === "light" || value === "dark" || value === "system"
    ? value
    : "system";
}

export function resolveThemePreference(
  preference: ThemePreference,
  systemDark: boolean,
): ResolvedTheme {
  return preference === "system"
    ? systemDark
      ? "dark"
      : "light"
    : preference;
}

export function themeColorFor(theme: ResolvedTheme) {
  return theme === "dark" ? "#171715" : "#f7f7f5";
}
