import { describe, expect, it } from "vitest";

import { getTextRenderFrame } from "@/features/workspace/text-render-frame";

describe("getTextRenderFrame", () => {
  it("does not constrain auto-width text to the draft transform box", () => {
    expect(getTextRenderFrame("autoWidth", 20, 6)).toEqual({
      wrap: "none",
    });
  });

  it("keeps fixed-frame text constrained and wrapped", () => {
    expect(getTextRenderFrame("fixedFrame", 20, 6)).toEqual({
      width: 20,
      height: 6,
      wrap: "word",
    });
  });
});
