export type BenchmarkFace = "front" | "back";

export const OBJECTS_PER_FACE = 100;
export const TOTAL_EDITABLE_OBJECTS = OBJECTS_PER_FACE * 2;
export const MAXIMUM_INACTIVE_DRAWS_PER_ACTIVE_CYCLE = 6;
export const MAXIMUM_INACTIVE_DRAW_RATIO = 0.02;

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
