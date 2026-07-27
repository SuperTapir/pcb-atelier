import { describe, expect, it } from "vitest";

import { getTextRenderFrame } from "@/features/workspace/text-render-frame";

describe("getTextRenderFrame", () => {
  it("centres auto-width text in its transform box without wrapping", () => {
    expect(getTextRenderFrame("autoWidth", 20, 6)).toEqual({
      align: "center",
      width: 20,
      height: 6,
      verticalAlign: "middle",
      wrap: "none",
    });
  });

  it("keeps fixed-frame text constrained and wrapped", () => {
    expect(getTextRenderFrame("fixedFrame", 20, 6)).toEqual({
      align: "center",
      width: 20,
      height: 6,
      verticalAlign: "middle",
      wrap: "word",
    });
  });
});
