import { describe, expect, it } from "vitest";

import {
  DEFAULT_APP_SETTINGS,
  loadAppSettings,
  saveAppSettings,
} from "@/features/settings/app-settings";

function memoryStorage(initial: string | null = null) {
  let value = initial;
  return {
    getItem: () => value,
    setItem: (_key: string, next: string) => {
      value = next;
    },
    value: () => value,
  };
}

describe("app settings", () => {
  it("defaults the dual-board arrangement to horizontal", () => {
    expect(loadAppSettings(memoryStorage())).toEqual(DEFAULT_APP_SETTINGS);
    expect(DEFAULT_APP_SETTINGS.canvasView).toBe("horizontal");
    expect(DEFAULT_APP_SETTINGS.wheelZoomDamping).toBe("medium");
    expect(DEFAULT_APP_SETTINGS.launchWindowMode).toBe("maximized");
    expect(DEFAULT_APP_SETTINGS.workspaceLeftPanelWidth).toBe(210);
    expect(DEFAULT_APP_SETTINGS.workspaceRightPanelWidth).toBe(300);
  });

  it("round-trips a valid arrangement and repairs invalid persisted data", () => {
    const storage = memoryStorage();
    saveAppSettings(
      {
        canvasView: "vertical",
        launchWindowMode: "fullscreen",
        wheelZoomDamping: "high",
        workspaceLeftPanelWidth: 360,
        workspaceRightPanelWidth: 440,
      },
      storage,
    );
    expect(loadAppSettings(storage).canvasView).toBe("vertical");
    expect(loadAppSettings(storage).wheelZoomDamping).toBe("high");
    expect(loadAppSettings(storage).launchWindowMode).toBe("fullscreen");
    expect(loadAppSettings(storage).workspaceLeftPanelWidth).toBe(360);
    expect(loadAppSettings(storage).workspaceRightPanelWidth).toBe(440);

    const invalid = memoryStorage(
      JSON.stringify({ canvasView: "diagonal" }),
    );
    expect(loadAppSettings(invalid)).toEqual(DEFAULT_APP_SETTINGS);
  });

  it("repairs persisted panel widths outside their supported ranges", () => {
    const storage = memoryStorage(
      JSON.stringify({
        ...DEFAULT_APP_SETTINGS,
        workspaceLeftPanelWidth: 20,
        workspaceRightPanelWidth: 4_000,
      }),
    );

    expect(loadAppSettings(storage)).toEqual({
      ...DEFAULT_APP_SETTINGS,
      workspaceLeftPanelWidth: 180,
      workspaceRightPanelWidth: 520,
    });
  });

  it("does not interpret the unreleased split layout settings", () => {
    const storage = memoryStorage(
      JSON.stringify({ boardArrangement: "auto", editLayout: "focus" }),
    );

    expect(loadAppSettings(storage)).toEqual(DEFAULT_APP_SETTINGS);
    expect(JSON.parse(storage.value() ?? "{}")).toEqual(DEFAULT_APP_SETTINGS);
  });
});
