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

  it("accepts one encoded PNG texture for each physical card face", () => {
    expect(validateBoardPreviewInput(previewInput())).toEqual([]);
  });

  it("rejects non-PNG texture payloads instead of guessing preview pixels", () => {
    const input = previewInput();
    input.textures.back.pngDataUrl = "data:image/jpeg;base64,fixture";

    expect(validateBoardPreviewInput(input)).toContain(
      "back texture must be a PNG data URL",
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
    fabricationInputSha256: "a".repeat(64),
    fabricationOutputSha256: "b".repeat(64),
    textures: {
      palette: {
        exposedCopper: { r: 211, g: 166, b: 57, a: 255 },
        solderMask: { r: 20, g: 105, b: 65, a: 255 },
        silkscreen: { r: 248, g: 246, b: 224, a: 255 },
        substrate: { r: 176, g: 132, b: 79, a: 255 },
      },
      front: {
        side: "front",
        widthPx: 2,
        heightPx: 2,
        pngDataUrl: "data:image/png;base64,front",
      },
      back: {
        side: "back",
        widthPx: 2,
        heightPx: 2,
        pngDataUrl: "data:image/png;base64,back",
      },
    },
  };
}
