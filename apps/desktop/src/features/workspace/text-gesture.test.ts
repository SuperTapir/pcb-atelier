import { describe, expect, it } from "vitest";

import { createTextDraft } from "@/features/workspace/text-gesture";

describe("createTextDraft", () => {
  it("creates point text for a click-sized gesture", () => {
    expect(createTextDraft({ x: 12, y: 18 }, { x: 12.4, y: 18.3 })).toEqual({
      layout: "autoWidth",
      xUm: 12_000,
      yUm: 18_000,
      widthUm: 20_000,
      heightUm: 6_000,
    });
  });

  it("creates a normalized fixed frame for a drag gesture", () => {
    expect(createTextDraft({ x: 30, y: 40 }, { x: 10, y: 20 })).toEqual({
      layout: "fixedFrame",
      xUm: 10_000,
      yUm: 20_000,
      widthUm: 20_000,
      heightUm: 20_000,
    });
  });
});
