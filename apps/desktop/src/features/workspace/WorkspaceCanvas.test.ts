import { describe, expect, it } from "vitest";

import {
  displayedXToBoardX,
  shouldClearCanvasSelection,
} from "@/features/workspace/WorkspaceCanvas";

describe("back-face view transform", () => {
  it("keeps both editing faces in upright physical coordinates", () => {
    expect(displayedXToBoardX(12, "front", 64)).toBe(12);
    expect(displayedXToBoardX(12, "back", 64)).toBe(12);
  });

  it("does not rewrite back-face domain coordinates for presentation", () => {
    const physicalX = 17;
    const displayedX = displayedXToBoardX(physicalX, "back", 64);
    expect(displayedX).toBe(physicalX);
  });
});

describe("canvas blank selection", () => {
  it("clears selection only on an already-active canvas with the select tool", () => {
    expect(shouldClearCanvasSelection("select", true, true)).toBe(true);
    expect(shouldClearCanvasSelection("select", true, false)).toBe(false);
    expect(shouldClearCanvasSelection("text", true, true)).toBe(false);
    expect(shouldClearCanvasSelection("select", false, true)).toBe(false);
  });
});
