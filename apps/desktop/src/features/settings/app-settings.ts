export interface AppSettings {
  canvasView: CanvasView;
  launchWindowMode: LaunchWindowMode;
  wheelZoomDamping: WheelZoomDamping;
  workspaceLeftPanelWidth: number;
  workspaceRightPanelWidth: number;
}

export type CanvasView = "horizontal" | "vertical" | "focus-active";
export type LaunchWindowMode = "maximized" | "fullscreen" | "windowed";
export type WheelZoomDamping = "high" | "medium" | "low";

export interface SettingsStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export const APP_SETTINGS_STORAGE_KEY = "pcb-atelier.app-settings.v2";

export const WORKSPACE_PANEL_WIDTH_LIMITS = {
  left: { min: 180, max: 420 },
  right: { min: 260, max: 520 },
} as const;

export const DEFAULT_APP_SETTINGS: AppSettings = {
  canvasView: "horizontal",
  launchWindowMode: "maximized",
  wheelZoomDamping: "medium",
  workspaceLeftPanelWidth: 210,
  workspaceRightPanelWidth: 300,
};

export function loadAppSettings(
  storage: SettingsStorage | undefined = globalThis.localStorage,
): AppSettings {
  if (!storage) return { ...DEFAULT_APP_SETTINGS };
  try {
    const raw = storage.getItem(APP_SETTINGS_STORAGE_KEY);
    if (!raw) return { ...DEFAULT_APP_SETTINGS };
    const parsed = JSON.parse(raw) as Partial<AppSettings>;
    const canvasView =
      parsed.canvasView === "horizontal" ||
      parsed.canvasView === "vertical" ||
      parsed.canvasView === "focus-active"
        ? parsed.canvasView
        : DEFAULT_APP_SETTINGS.canvasView;
    const wheelZoomDamping =
      parsed.wheelZoomDamping === "high" ||
      parsed.wheelZoomDamping === "medium" ||
      parsed.wheelZoomDamping === "low"
        ? parsed.wheelZoomDamping
        : DEFAULT_APP_SETTINGS.wheelZoomDamping;
    const launchWindowMode =
      parsed.launchWindowMode === "maximized" ||
      parsed.launchWindowMode === "fullscreen" ||
      parsed.launchWindowMode === "windowed"
        ? parsed.launchWindowMode
        : DEFAULT_APP_SETTINGS.launchWindowMode;
    const workspaceLeftPanelWidth = clampPanelWidth(
      parsed.workspaceLeftPanelWidth,
      WORKSPACE_PANEL_WIDTH_LIMITS.left,
      DEFAULT_APP_SETTINGS.workspaceLeftPanelWidth,
    );
    const workspaceRightPanelWidth = clampPanelWidth(
      parsed.workspaceRightPanelWidth,
      WORKSPACE_PANEL_WIDTH_LIMITS.right,
      DEFAULT_APP_SETTINGS.workspaceRightPanelWidth,
    );
    const repaired = {
      canvasView,
      launchWindowMode,
      wheelZoomDamping,
      workspaceLeftPanelWidth,
      workspaceRightPanelWidth,
    };
    if (
      canvasView !== parsed.canvasView ||
      launchWindowMode !== parsed.launchWindowMode ||
      wheelZoomDamping !== parsed.wheelZoomDamping ||
      workspaceLeftPanelWidth !== parsed.workspaceLeftPanelWidth ||
      workspaceRightPanelWidth !== parsed.workspaceRightPanelWidth
    ) {
      saveAppSettings(repaired, storage);
    }
    return repaired;
  } catch {
    // Invalid or unavailable local storage falls back to stable defaults.
  }
  return { ...DEFAULT_APP_SETTINGS };
}

export function saveAppSettings(
  settings: AppSettings,
  storage: SettingsStorage | undefined = globalThis.localStorage,
) {
  if (!storage) return;
  try {
    storage.setItem(APP_SETTINGS_STORAGE_KEY, JSON.stringify(settings));
  } catch {
    // The editor stays usable when storage is disabled or full.
  }
}

function clampPanelWidth(
  value: unknown,
  limits: { min: number; max: number },
  fallback: number,
) {
  if (typeof value !== "number" || !Number.isFinite(value)) return fallback;
  return Math.round(Math.min(limits.max, Math.max(limits.min, value)));
}
