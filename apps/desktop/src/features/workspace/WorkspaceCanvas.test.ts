import { describe, expect, it } from "vitest";

import { displayedXToBoardX } from "@/features/workspace/WorkspaceCanvas";

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
