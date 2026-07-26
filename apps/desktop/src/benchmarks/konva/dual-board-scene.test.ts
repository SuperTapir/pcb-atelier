import { describe, expect, it } from "vitest";

import {
  createDualBoardObjects,
  evaluateInactiveBoardDraws,
  type BoardDrawSample,
} from "./dual-board-scene";

describe("dual-board Konva benchmark scene", () => {
  it("keeps both faces mounted with 100 objects each and 200 in total", () => {
    const scene = createDualBoardObjects();

    expect(scene.front).toHaveLength(100);
    expect(scene.back).toHaveLength(100);
    expect([...scene.front, ...scene.back]).toHaveLength(200);
    expect(new Set([...scene.front, ...scene.back].map((item) => item.id)).size).toBe(
      200,
    );
  });

  it("passes when the inactive board does not redraw continuously", () => {
    const samples: BoardDrawSample[] = [
      { activeFace: "front", activeDraws: 540, inactiveDraws: 3 },
      { activeFace: "back", activeDraws: 540, inactiveDraws: 0 },
    ];

    expect(evaluateInactiveBoardDraws(samples)).toMatchObject({
      totalInactiveDraws: 3,
      passed: true,
    });
  });

  it("fails when an inactive board redraws along with every active frame", () => {
    const samples: BoardDrawSample[] = [
      { activeFace: "front", activeDraws: 540, inactiveDraws: 540 },
    ];

    expect(evaluateInactiveBoardDraws(samples).passed).toBe(false);
  });

  it("fails an empty sample that did not exercise the active board", () => {
    expect(
      evaluateInactiveBoardDraws([
        { activeFace: "front", activeDraws: 0, inactiveDraws: 0 },
      ]).passed,
    ).toBe(false);
  });
});
