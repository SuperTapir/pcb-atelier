import { describe, expect, it } from "vitest";

import {
  normalizeThemePreference,
  resolveThemePreference,
  themeColorFor,
} from "@/features/theme/theme-state";

describe("theme state", () => {
  it("defaults missing or invalid persisted values to system", () => {
    expect(normalizeThemePreference(null)).toBe("system");
    expect(normalizeThemePreference("sepia")).toBe("system");
    expect(normalizeThemePreference("dark")).toBe("dark");
  });

  it("resolves system preference without overriding explicit choices", () => {
    expect(resolveThemePreference("system", true)).toBe("dark");
    expect(resolveThemePreference("system", false)).toBe("light");
    expect(resolveThemePreference("light", true)).toBe("light");
    expect(resolveThemePreference("dark", false)).toBe("dark");
  });

  it("provides matching native window colors for both resolved themes", () => {
    expect(themeColorFor("light")).toBe("#f7f7f5");
    expect(themeColorFor("dark")).toBe("#171715");
  });
});
