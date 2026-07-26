import { describe, expect, it } from "vitest";

import {
  createProductionInspectionState,
  isProductionLayerRendered,
  toggleProductionIsolation,
  toggleProductionVisibility,
} from "@/features/workspace/production-inspection";

describe("production inspection state", () => {
  it("shows all physical production layers by default", () => {
    const state = createProductionInspectionState();

    expect(state.front.copper.visible).toBe(true);
    expect(state.front.solderMaskOpen.visible).toBe(true);
    expect(state.front.silkscreen.visible).toBe(true);
    expect(state.back.copper.visible).toBe(true);
    expect(state.back.solderMaskOpen.visible).toBe(true);
    expect(state.back.silkscreen.visible).toBe(true);
  });

  it("keeps temporary visibility independent per physical face", () => {
    const state = toggleProductionVisibility(
      createProductionInspectionState(),
      "back",
      "copper",
    );
    expect(state.front.copper.visible).toBe(true);
    expect(state.back.copper.visible).toBe(false);
  });

  it("isolates one layer without changing export participation", () => {
    const state = toggleProductionIsolation(
      createProductionInspectionState(),
      "front",
      "silkscreen",
    );
    expect(isProductionLayerRendered(state.front, "silkscreen")).toBe(true);
    expect(isProductionLayerRendered(state.front, "copper")).toBe(false);
    expect(state.front.copper.visible).toBe(true);
  });
});
