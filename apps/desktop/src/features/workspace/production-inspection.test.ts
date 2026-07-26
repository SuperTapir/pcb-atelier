import { describe, expect, it } from "vitest";

import {
  createProductionInspectionState,
  isProductionLayerRendered,
  toggleProductionIsolation,
  toggleProductionVisibility,
} from "@/features/workspace/production-inspection";

describe("production inspection state", () => {
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
