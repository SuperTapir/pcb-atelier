import { describe, expect, it } from "vitest";

import {
  createInitialWorkspaceState,
  workspaceReducer,
} from "@/features/workspace/workspace-state";

describe("workspaceReducer", () => {
  it("defaults dual boards to the persisted horizontal preference", () => {
    expect(createInitialWorkspaceState().boardArrangement).toBe("horizontal");
    expect(createInitialWorkspaceState("vertical").boardArrangement).toBe(
      "vertical",
    );
    expect(createInitialWorkspaceState("horizontal", "focus").editLayout).toBe(
      "focus",
    );
  });

  it("keeps workspace mode, edit layout, active face and tool orthogonal", () => {
    const state = [
      { type: "setWorkspaceMode", workspaceMode: "preview" } as const,
      { type: "setEditLayout", editLayout: "focus" } as const,
      {
        type: "setBoardArrangement",
        boardArrangement: "horizontal",
      } as const,
      { type: "setFace", face: "back" } as const,
      { type: "setTool", tool: "text" } as const,
    ].reduce(workspaceReducer, createInitialWorkspaceState());

    expect(state.workspaceMode).toBe("preview");
    expect(state.editLayout).toBe("focus");
    expect(state.boardArrangement).toBe("horizontal");
    expect(state.activeFace).toBe("back");
    expect(state.tool).toBe("text");
  });

  it("recenters pixel-space pan when arrangement changes without losing zoom or selection", () => {
    let state = createInitialWorkspaceState();
    state = workspaceReducer(state, {
      type: "setSelection",
      face: "front",
      layerIds: ["front-title"],
    });
    state = workspaceReducer(state, {
      type: "setViewport",
      face: "back",
      viewport: { zoom: 1.4, panX: 20, panY: -10 },
    });
    state = workspaceReducer(state, {
      type: "setBoardArrangement",
      boardArrangement: "vertical",
    });

    expect(state.boardArrangement).toBe("vertical");
    expect(state.selections.front).toEqual(["front-title"]);
    expect(state.viewports.back).toEqual({
      zoom: 1.4,
      panX: 0,
      panY: 0,
    });
  });

  it("does not lose either face selection when mode, layout or active face changes", () => {
    let state = createInitialWorkspaceState();
    state = workspaceReducer(state, {
      type: "setSelection",
      face: "front",
      layerIds: ["front-title"],
    });
    state = workspaceReducer(state, {
      type: "setSelection",
      face: "back",
      layerIds: ["back-mark"],
    });
    state = workspaceReducer(state, {
      type: "setWorkspaceMode",
      workspaceMode: "preview",
    });
    state = workspaceReducer(state, {
      type: "setEditLayout",
      editLayout: "focus",
    });
    state = workspaceReducer(state, { type: "setFace", face: "back" });

    expect(state.selections.front).toEqual(["front-title"]);
    expect(state.selections.back).toEqual(["back-mark"]);
  });

  it("keeps an independent work context for each face across mode and layout changes", () => {
    let state = createInitialWorkspaceState();
    state = workspaceReducer(state, {
      type: "setWorkContext",
      face: "front",
      workContext: "silkscreen",
    });
    state = workspaceReducer(state, {
      type: "setWorkContext",
      face: "back",
      workContext: "solderMaskOpen",
    });
    state = workspaceReducer(state, {
      type: "setWorkspaceMode",
      workspaceMode: "preview",
    });
    state = workspaceReducer(state, {
      type: "setEditLayout",
      editLayout: "focus",
    });
    state = workspaceReducer(state, { type: "setFace", face: "back" });

    expect(state.workContexts.front).toBe("silkscreen");
    expect(state.workContexts.back).toBe("solderMaskOpen");
  });

  it("defaults inserts to a real production layer and keeps board selection explicit", () => {
    let state = createInitialWorkspaceState();
    expect(state.workContexts).toEqual({
      front: "silkscreen",
      back: "silkscreen",
    });

    state = workspaceReducer(state, { type: "selectBoard" });
    expect(state.inspectorTarget).toBe("board");

    state = workspaceReducer(state, {
      type: "setWorkContext",
      face: "back",
      workContext: "copper",
    });
    expect(state.activeFace).toBe("back");
    expect(state.inspectorTarget).toBe("face");
  });

  it("does not lose either face viewport when mode, layout or active face changes", () => {
    const frontViewport = { zoom: 1.6, panX: 48, panY: -24 };
    const backViewport = { zoom: 0.8, panX: -12, panY: 32 };
    let state = createInitialWorkspaceState();
    state = workspaceReducer(state, {
      type: "setViewport",
      face: "front",
      viewport: frontViewport,
    });
    state = workspaceReducer(state, {
      type: "setViewport",
      face: "back",
      viewport: backViewport,
    });
    state = workspaceReducer(state, {
      type: "setWorkspaceMode",
      workspaceMode: "preview",
    });
    state = workspaceReducer(state, {
      type: "setEditLayout",
      editLayout: "focus",
    });
    state = workspaceReducer(state, { type: "setFace", face: "back" });

    expect(state.viewports.front).toEqual(frontViewport);
    expect(state.viewports.back).toEqual(backViewport);
  });

  it("resets and clamps only the addressed face viewport", () => {
    let state = createInitialWorkspaceState();
    state = workspaceReducer(state, {
      type: "setViewport",
      face: "front",
      viewport: { zoom: 99, panX: 7, panY: 8 },
    });
    state = workspaceReducer(state, {
      type: "setViewport",
      face: "back",
      viewport: { zoom: 0.8, panX: 12, panY: 13 },
    });
    state = workspaceReducer(state, {
      type: "resetViewport",
      face: "front",
    });

    expect(state.viewports.front).toEqual({ zoom: 1, panX: 0, panY: 0 });
    expect(state.viewports.back).toEqual({
      zoom: 0.8,
      panX: 12,
      panY: 13,
    });
  });
});
