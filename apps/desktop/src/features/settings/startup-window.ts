import type { AppSettings } from "@/features/settings/app-settings";

interface FullscreenWindow {
  maximize(): Promise<void>;
  setFullscreen(fullscreen: boolean): Promise<void>;
  unmaximize(): Promise<void>;
}

async function getDesktopWindow(): Promise<FullscreenWindow | null> {
  if (!("__TAURI_INTERNALS__" in globalThis)) return null;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}

export async function applyStartupWindowPreference<
  T extends { launchWindowMode: AppSettings["launchWindowMode"] },
>(
  settings: T,
  getWindow: () => Promise<FullscreenWindow | null> = getDesktopWindow,
) {
  const window = await getWindow();
  if (!window) return;
  if (settings.launchWindowMode === "fullscreen") {
    await window.setFullscreen(true);
    return;
  }
  await window.setFullscreen(false);
  if (settings.launchWindowMode === "maximized") {
    await window.maximize();
  } else {
    await window.unmaximize();
  }
}
