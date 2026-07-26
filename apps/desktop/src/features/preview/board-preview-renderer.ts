import type { ComponentType } from "react";

export type PreviewCardSide = "front" | "back";

export type BoardPreviewOutline =
  | {
      type: "rectangle";
      widthUm: number;
      heightUm: number;
    }
  | {
      type: "roundedRectangle";
      widthUm: number;
      heightUm: number;
      cornerRadiusUm: number;
    };

/**
 * TypeScript projection of atelier-core's `PreviewTexture`.
 *
 * The encoded texture is produced by Core. Renderers may only display these
 * pixels and never infer manufacturing output from editor objects.
 */
export interface BoardPreviewTexture {
  side: PreviewCardSide;
  widthPx: number;
  heightPx: number;
  pngDataUrl: string;
}

/**
 * Readonly data boundary for a complete board preview.
 *
 * `textures` corresponds to Core's `ResolvedPreviewTextures`; the board
 * outline and thickness correspond to its `ResolvedFabricationBoard`.
 */
export interface BoardPreviewInput {
  outline: BoardPreviewOutline;
  thicknessUm: number;
  fabricationInputSha256: string;
  fabricationOutputSha256: string;
  textures: {
    palette: {
      solderMask: { r: number; g: number; b: number; a: number };
    };
    front: BoardPreviewTexture;
    back: BoardPreviewTexture;
  };
}

export interface BoardPreviewGeometry {
  widthMm: number;
  heightMm: number;
  thicknessMm: number;
  cornerRadiusMm: number;
}

export interface BoardPreviewRendererProps {
  preview: BoardPreviewInput;
  className?: string;
}

export interface BoardPreviewRenderer {
  readonly id: string;
  readonly Component: ComponentType<BoardPreviewRendererProps>;
}

export function getBoardPreviewGeometry(
  preview: BoardPreviewInput,
): BoardPreviewGeometry {
  return {
    widthMm: preview.outline.widthUm / 1_000,
    heightMm: preview.outline.heightUm / 1_000,
    thicknessMm: preview.thicknessUm / 1_000,
    cornerRadiusMm:
      preview.outline.type === "roundedRectangle"
        ? preview.outline.cornerRadiusUm / 1_000
        : 0,
  };
}

export function validateBoardPreviewInput(
  preview: BoardPreviewInput,
): string[] {
  const errors: string[] = [];
  const geometry = getBoardPreviewGeometry(preview);

  if (geometry.widthMm <= 0 || geometry.heightMm <= 0) {
    errors.push("board width and height must be positive");
  }
  if (geometry.thicknessMm <= 0) {
    errors.push("board thickness must be positive");
  }
  if (!/^[a-f0-9]{64}$/i.test(preview.fabricationInputSha256)) {
    errors.push("fabrication input hash must be a SHA-256 digest");
  }
  if (!/^[a-f0-9]{64}$/i.test(preview.fabricationOutputSha256)) {
    errors.push("fabrication output hash must be a SHA-256 digest");
  }
  if (
    geometry.cornerRadiusMm < 0 ||
    geometry.cornerRadiusMm >
      Math.min(geometry.widthMm, geometry.heightMm) / 2
  ) {
    errors.push("board corner radius is outside the outline");
  }

  validateTexture(preview.textures.front, "front", errors);
  validateTexture(preview.textures.back, "back", errors);
  return errors;
}

function validateTexture(
  texture: BoardPreviewTexture,
  expectedSide: PreviewCardSide,
  errors: string[],
) {
  if (texture.side !== expectedSide) {
    errors.push(
      `${expectedSide} texture declares physical side ${texture.side}`,
    );
  }
  if (
    !Number.isInteger(texture.widthPx) ||
    !Number.isInteger(texture.heightPx) ||
    texture.widthPx <= 0 ||
    texture.heightPx <= 0
  ) {
    errors.push(`${expectedSide} texture dimensions must be positive integers`);
    return;
  }
  if (!texture.pngDataUrl.startsWith("data:image/png;base64,")) {
    errors.push(`${expectedSide} texture must be a PNG data URL`);
  }
}
