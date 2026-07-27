import { describe, expect, it } from "vitest";

import {
  createDualBoardObjects,
  evaluateGesturePerformance,
  evaluateInactiveBoardDraws,
  MAXIMUM_GESTURE_UPDATE_P95_MS,
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
      { activeFace: "front", activeDraws: 540, inactiveDraws: 0 },
      { activeFace: "back", activeDraws: 540, inactiveDraws: 0 },
    ];

    expect(evaluateInactiveBoardDraws(samples)).toMatchObject({
      totalInactiveDraws: 0,
      passed: true,
    });
  });

  it("rejects even one inactive-canvas redraw during an active gesture", () => {
    expect(
      evaluateInactiveBoardDraws([
        { activeFace: "front", activeDraws: 180, inactiveDraws: 1 },
      ]).passed,
    ).toBe(false);
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

describe("200-object gesture performance contract", () => {
  it("uses the OpenSpec p95 target without rounding it up", () => {
    expect(MAXIMUM_GESTURE_UPDATE_P95_MS).toBe(16.7);
    expect(
      evaluateGesturePerformance({
        editableObjects: 200,
        updateDurationsMs: [5, 8, 12, 16.7],
        productionCompileCalls: 0,
        ipcCalls: 0,
        synchronousIpcCalls: 0,
      }).passed,
    ).toBe(true);
    expect(
      evaluateGesturePerformance({
        editableObjects: 200,
        updateDurationsMs: [16.71],
        productionCompileCalls: 0,
        ipcCalls: 0,
        synchronousIpcCalls: 0,
      }).passed,
    ).toBe(false);
  });

  it("fails without 200 objects or when gestures compile production proxies or invoke IPC", () => {
    for (const sample of [
      {
        editableObjects: 199,
        productionCompileCalls: 0,
        ipcCalls: 0,
        synchronousIpcCalls: 0,
      },
      {
        editableObjects: 200,
        productionCompileCalls: 1,
        ipcCalls: 0,
        synchronousIpcCalls: 0,
      },
      {
        editableObjects: 200,
        productionCompileCalls: 0,
        ipcCalls: 1,
        synchronousIpcCalls: 0,
      },
      {
        editableObjects: 200,
        productionCompileCalls: 0,
        ipcCalls: 0,
        synchronousIpcCalls: 1,
      },
    ]) {
      expect(
        evaluateGesturePerformance({
          ...sample,
          updateDurationsMs: [4, 6, 8],
        }).passed,
      ).toBe(false);
    }
  });
});
