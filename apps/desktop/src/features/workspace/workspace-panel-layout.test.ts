import { describe, expect, it } from "vitest";

import {
  resizeWorkspacePanelWidth,
  WORKSPACE_PANEL_WIDTH_LIMITS,
} from "@/features/workspace/workspace-panel-layout";

describe("workspace panel resize limits", () => {
  it("resizes left and right panels in their natural drag directions", () => {
    expect(
      resizeWorkspacePanelWidth("left", 210, 80, 300, 1_440),
    ).toBe(290);
    expect(
      resizeWorkspacePanelWidth("right", 300, -80, 210, 1_440),
    ).toBe(380);
  });

  it("clamps both panels to usable minimum and maximum widths", () => {
    expect(
      resizeWorkspacePanelWidth("left", 210, -1_000, 300, 1_440),
    ).toBe(WORKSPACE_PANEL_WIDTH_LIMITS.left.min);
    expect(
      resizeWorkspacePanelWidth("left", 210, 1_000, 300, 1_440),
    ).toBe(WORKSPACE_PANEL_WIDTH_LIMITS.left.max);
    expect(
      resizeWorkspacePanelWidth("right", 300, 1_000, 210, 1_440),
    ).toBe(WORKSPACE_PANEL_WIDTH_LIMITS.right.min);
    expect(
      resizeWorkspacePanelWidth("right", 300, -1_000, 210, 1_440),
    ).toBe(WORKSPACE_PANEL_WIDTH_LIMITS.right.max);
  });

  it("does not enlarge a panel into the minimum canvas budget", () => {
    expect(
      resizeWorkspacePanelWidth("left", 210, 300, 300, 1_100),
    ).toBe(268);
    expect(
      resizeWorkspacePanelWidth("right", 300, -300, 210, 1_100),
    ).toBe(358);
  });
});
