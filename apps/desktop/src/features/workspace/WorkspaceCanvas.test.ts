import { describe, expect, it } from "vitest";

import { displayedXToBoardX } from "@/features/workspace/WorkspaceCanvas";

describe("back-face view transform", () => {
  it("mirrors only the displayed X coordinate", () => {
    expect(displayedXToBoardX(12, "front", 64)).toBe(12);
    expect(displayedXToBoardX(12, "back", 64)).toBe(52);
  });

  it("is its own inverse for the back view", () => {
    const physicalX = 17;
    const displayedX = displayedXToBoardX(physicalX, "back", 64);
    expect(displayedXToBoardX(displayedX, "back", 64)).toBe(physicalX);
  });
});
