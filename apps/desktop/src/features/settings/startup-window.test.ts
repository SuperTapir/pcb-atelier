import { describe, expect, it, vi } from "vitest";

import { applyStartupWindowPreference } from "@/features/settings/startup-window";

describe("startup window preference", () => {
  it("enters system fullscreen when the persisted launch preference requests it", async () => {
    const setFullscreen = vi.fn(async () => {});
    const maximize = vi.fn(async () => {});
    const unmaximize = vi.fn(async () => {});

    await applyStartupWindowPreference(
      {
        canvasView: "horizontal",
        launchWindowMode: "fullscreen",
        wheelZoomDamping: "medium",
      },
      async () => ({ maximize, setFullscreen, unmaximize }),
    );

    expect(setFullscreen).toHaveBeenCalledOnce();
    expect(setFullscreen).toHaveBeenCalledWith(true);
  });

  it("defaults to windowed fullscreen by maximizing without entering system fullscreen", async () => {
    const setFullscreen = vi.fn(async () => {});
    const maximize = vi.fn(async () => {});
    const unmaximize = vi.fn(async () => {});

    await applyStartupWindowPreference(
      {
        canvasView: "horizontal",
        launchWindowMode: "maximized",
        wheelZoomDamping: "medium",
      },
      async () => ({ maximize, setFullscreen, unmaximize }),
    );

    expect(setFullscreen).toHaveBeenCalledWith(false);
    expect(maximize).toHaveBeenCalledOnce();
    expect(unmaximize).not.toHaveBeenCalled();
  });
});
