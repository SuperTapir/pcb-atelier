import { renderToStaticMarkup } from "react-dom/server";
import {
  LinearFilter,
  LinearMipmapLinearFilter,
  MeshStandardMaterial,
  SRGBColorSpace,
  Texture,
} from "three";
import { describe, expect, it } from "vitest";

import {
  Board3DPreview,
  board3DPreviewRenderer,
  configureBoardFaceMaterial,
  configureBoardTexture,
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

  it("uses mipmaps and anisotropic sampling during oblique interaction", () => {
    const texture = new Texture();

    configureBoardTexture(texture, 16);

    expect(texture.colorSpace).toBe(SRGBColorSpace);
    expect(texture.magFilter).toBe(LinearFilter);
    expect(texture.minFilter).toBe(LinearMipmapLinearFilter);
    expect(texture.generateMipmaps).toBe(true);
    expect(texture.anisotropy).toBe(8);
    expect(texture.version).toBeGreaterThan(0);
  });

  it("keeps textured faces in front of body caps at grazing angles", () => {
    const material = new MeshStandardMaterial();

    configureBoardFaceMaterial(material);

    expect(material.polygonOffset).toBe(true);
    expect(material.polygonOffsetFactor).toBe(-2);
    expect(material.polygonOffsetUnits).toBe(-2);
    expect(material.version).toBeGreaterThan(0);
    material.dispose();
  });
});

function previewInput(): BoardPreviewInput {
  const pngDataUrl = "data:image/png;base64,fixture";
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
      palette: { solderMask: { r: 35, g: 67, b: 135, a: 255 } },
      front: { side: "front", widthPx: 2, heightPx: 2, pngDataUrl },
      back: { side: "back", widthPx: 2, heightPx: 2, pngDataUrl },
    },
  };
}
