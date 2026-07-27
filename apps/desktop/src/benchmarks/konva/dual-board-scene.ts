export type BenchmarkFace = "front" | "back";

export const OBJECTS_PER_FACE = 100;
export const TOTAL_EDITABLE_OBJECTS = OBJECTS_PER_FACE * 2;
export const MAXIMUM_INACTIVE_DRAWS_PER_ACTIVE_CYCLE = 0;
export const MAXIMUM_INACTIVE_DRAW_RATIO = 0;
export const MAXIMUM_GESTURE_UPDATE_P95_MS = 16.7;

export type EditableObject =
  | {
      id: string;
      kind: "rect";
      x: number;
      y: number;
      width: number;
      height: number;
      color: string;
    }
  | {
      id: string;
      kind: "text";
      x: number;
      y: number;
      text: string;
      color: string;
    }
  | {
      id: string;
      kind: "image";
      x: number;
      y: number;
      width: number;
      height: number;
      imageIndex: number;
    };

export interface BoardDrawSample {
  activeFace: BenchmarkFace;
  activeDraws: number;
  inactiveDraws: number;
}

export interface InactiveBoardDrawResult {
  samples: BoardDrawSample[];
  totalActiveDraws: number;
  totalInactiveDraws: number;
  inactiveDrawRatio: number;
  passed: boolean;
}

export interface GesturePerformanceSample {
  editableObjects: number;
  updateDurationsMs: number[];
  productionCompileCalls: number;
  ipcCalls: number;
  synchronousIpcCalls: number;
}

export interface GesturePerformanceResult {
  editableObjects: number;
  measuredUpdates: number;
  productionCompileCalls: number;
  ipcCalls: number;
  synchronousIpcCalls: number;
  averageMs: number;
  p95Ms: number;
  maximumMs: number;
  checks: {
    objectCount: boolean;
    measured: boolean;
    p95: boolean;
    noProductionCompile: boolean;
    noIpc: boolean;
    noSynchronousIpc: boolean;
  };
  passed: boolean;
}

export function createDualBoardObjects(
  board = { width: 410, height: 540 },
): Record<BenchmarkFace, EditableObject[]> {
  return {
    front: makeFaceObjects("front", board.width, board.height, 0),
    back: makeFaceObjects("back", board.width, board.height, 17),
  };
}

export function evaluateInactiveBoardDraws(
  samples: BoardDrawSample[],
): InactiveBoardDrawResult {
  const totalActiveDraws = samples.reduce(
    (sum, sample) => sum + sample.activeDraws,
    0,
  );
  const totalInactiveDraws = samples.reduce(
    (sum, sample) => sum + sample.inactiveDraws,
    0,
  );
  const inactiveDrawRatio =
    totalInactiveDraws / Math.max(totalActiveDraws, 1);
  return {
    samples,
    totalActiveDraws,
    totalInactiveDraws,
    inactiveDrawRatio,
    passed:
      samples.every(
        (sample) =>
          sample.activeDraws > 0 &&
          sample.inactiveDraws <= MAXIMUM_INACTIVE_DRAWS_PER_ACTIVE_CYCLE,
      ) && inactiveDrawRatio <= MAXIMUM_INACTIVE_DRAW_RATIO,
  };
}

export function evaluateGesturePerformance(
  sample: GesturePerformanceSample,
): GesturePerformanceResult {
  const averageMs =
    sample.updateDurationsMs.reduce((sum, value) => sum + value, 0) /
    Math.max(sample.updateDurationsMs.length, 1);
  const p95Ms = percentile(sample.updateDurationsMs, 0.95);
  const checks = {
    objectCount: sample.editableObjects === TOTAL_EDITABLE_OBJECTS,
    measured: sample.updateDurationsMs.length > 0,
    p95: p95Ms <= MAXIMUM_GESTURE_UPDATE_P95_MS,
    noProductionCompile: sample.productionCompileCalls === 0,
    noIpc: sample.ipcCalls === 0,
    noSynchronousIpc: sample.synchronousIpcCalls === 0,
  };
  return {
    editableObjects: sample.editableObjects,
    measuredUpdates: sample.updateDurationsMs.length,
    productionCompileCalls: sample.productionCompileCalls,
    ipcCalls: sample.ipcCalls,
    synchronousIpcCalls: sample.synchronousIpcCalls,
    averageMs,
    p95Ms,
    maximumMs: Math.max(0, ...sample.updateDurationsMs),
    checks,
    passed: Object.values(checks).every(Boolean),
  };
}

function makeFaceObjects(
  face: BenchmarkFace,
  width: number,
  height: number,
  offset: number,
): EditableObject[] {
  const objects: EditableObject[] = [];
  for (let index = 0; index < 60; index += 1) {
    objects.push({
      id: `${face}-rect-${index}`,
      kind: "rect",
      x: 14 + (((index + offset) * 47) % (width - 64)),
      y: 14 + (((index + offset) * 41) % (height - 56)),
      width: 20 + (index % 5) * 5,
      height: 14 + (index % 4) * 5,
      color: index % 2 ? "#e9e0c7" : "#cf9f37",
    });
  }
  for (let index = 0; index < 24; index += 1) {
    objects.push({
      id: `${face}-text-${index}`,
      kind: "text",
      x: 16 + (((index + offset) * 71) % (width - 105)),
      y: 16 + (((index + offset) * 59) % (height - 44)),
      text: `${face === "front" ? "F" : "B"} ART ${String(index + 1).padStart(2, "0")}`,
      color: index % 2 ? "#e8bd5f" : "#fff7df",
    });
  }
  for (let index = 0; index < 16; index += 1) {
    objects.push({
      id: `${face}-image-${index}`,
      kind: "image",
      x: 18 + (((index + offset) * 83) % (width - 112)),
      y: 18 + (((index + offset) * 67) % (height - 86)),
      width: 76,
      height: 48,
      imageIndex: index % 8,
    });
  }
  return objects;
}

function percentile(values: number[], fraction: number) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.ceil(sorted.length * fraction) - 1] ?? 0;
}
