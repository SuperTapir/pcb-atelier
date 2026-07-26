import { describe, expect, it } from "vitest";

import {
  getBoardPreviewGeometry,
  validateBoardPreviewInput,
  type BoardPreviewInput,
} from "@/features/preview/board-preview-renderer";

describe("BoardPreviewInput", () => {
  it("keeps physical board proportions and thickness in millimetres", () => {
    const input = previewInput();

    expect(getBoardPreviewGeometry(input)).toEqual({
      widthMm: 64,
      heightMm: 100,
      thicknessMm: 1.6,
      cornerRadiusMm: 2,
    });
  });

  it("accepts one exact RGBA texture for each physical card face", () => {
    expect(validateBoardPreviewInput(previewInput())).toEqual([]);
  });

  it("rejects malformed texture buffers instead of guessing preview pixels", () => {
    const input = previewInput();
    input.textures.back.rgba.pop();

    expect(validateBoardPreviewInput(input)).toContain(
      "back texture has 15 RGBA bytes; expected 16",
    );
  });
});

function previewInput(): BoardPreviewInput {
  return {
    outline: {
      type: "roundedRectangle",
      widthUm: 64_000,
      heightUm: 100_000,
      cornerRadiusUm: 2_000,
    },
    thicknessUm: 1_600,
    textures: {
      front: {
        side: "front",
        widthPx: 2,
        heightPx: 2,
        rgba: [
          20, 105, 65, 255, 20, 105, 65, 255, 20, 105, 65, 255, 20, 105,
          65, 255,
        ],
      },
      back: {
        side: "back",
        widthPx: 2,
        heightPx: 2,
        rgba: [
          20, 105, 65, 255, 20, 105, 65, 255, 20, 105, 65, 255, 20, 105,
          65, 255,
        ],
      },
    },
  };
}
