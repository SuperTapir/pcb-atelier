import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import {
  Board3DPreview,
  board3DPreviewRenderer,
} from "@/features/preview/Board3DPreview";
import type { BoardPreviewInput } from "@/features/preview/board-preview-renderer";

describe("Board3DPreview", () => {
  it("exposes a stable readonly preview surface contract", () => {
    const markup = renderToStaticMarkup(
      <Board3DPreview preview={previewInput()} />,
    );

    expect(markup).toContain('data-testid="board-3d-preview"');
    expect(markup).toContain('aria-label="3D 成板效果预览"');
    expect(markup).not.toContain("input");
    expect(markup).not.toContain("textarea");
  });

  it("is available through the replaceable renderer interface", () => {
    expect(board3DPreviewRenderer.id).toBe("three-board-preview");
    expect(board3DPreviewRenderer.Component).toBe(Board3DPreview);
  });
});

function previewInput(): BoardPreviewInput {
  const rgba = [
    35, 67, 135, 255, 35, 67, 135, 255, 35, 67, 135, 255, 35, 67, 135, 255,
  ];
  return {
    outline: {
      type: "roundedRectangle",
      widthUm: 64_000,
      heightUm: 100_000,
      cornerRadiusUm: 2_000,
    },
    thicknessUm: 1_600,
    textures: {
      front: { side: "front", widthPx: 2, heightPx: 2, rgba },
      back: { side: "back", widthPx: 2, heightPx: 2, rgba },
    },
  };
}
