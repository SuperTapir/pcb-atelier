import type { ComponentType } from "react";

import type { BoardPreviewOutline } from "@/features/preview/board-preview-renderer";
import type { ProductionLayer } from "@/lib/core";

export type ProductionSide = "front" | "back";

export interface ProductionLayerTexture {
  side: ProductionSide;
  layer: ProductionLayer;
  widthPx: number;
  heightPx: number;
  rgba: number[];
}

export interface ProductionPreviewInput {
  source: "resolvedFabricationBoard";
  outline: BoardPreviewOutline;
  fabricationInputSha256: string;
  fabricationOutputSha256: string;
  pixelPitchUm: number;
  textures: ProductionLayerTexture[];
}

export type ProductionLayerVisibility = Record<ProductionLayer, boolean>;

export interface ProductionLayerSelection {
  side: ProductionSide;
  visibility?: Partial<ProductionLayerVisibility>;
  isolatedLayer?: ProductionLayer | null;
  mirroredForViewing?: boolean;
}

export interface ProductionLayerViewTransform {
  scaleX: 1 | -1;
  xUm: number;
}

export interface ProductionRenderLayer {
  side: ProductionSide;
  layer: ProductionLayer;
  texture: ProductionLayerTexture;
  viewTransform: ProductionLayerViewTransform;
}

export interface ProductionRendererProps {
  preview: ProductionPreviewInput;
  selection: ProductionLayerSelection;
  className?: string;
}

export interface ProductionRenderer {
  readonly id: string;
  readonly Component: ComponentType<ProductionRendererProps>;
}

export function selectProductionTexture(
  preview: ProductionPreviewInput,
  side: ProductionSide,
  layer: ProductionLayer,
) {
  return preview.textures.find(
    (texture) => texture.side === side && texture.layer === layer,
  );
}

export function validateProductionPreview(
  preview: ProductionPreviewInput,
): string[] {
  const errors: string[] = [];
  const expected: Array<[ProductionSide, ProductionLayer]> = [
    ["front", "copper"],
    ["front", "solderMaskOpen"],
    ["front", "silkscreen"],
    ["back", "copper"],
    ["back", "solderMaskOpen"],
    ["back", "silkscreen"],
  ];

  if (preview.source !== "resolvedFabricationBoard") {
    errors.push("preview source must be ResolvedFabricationBoard");
  }
  if (!isSha256(preview.fabricationInputSha256)) {
    errors.push("fabrication input hash must be a SHA-256 digest");
  }
  if (!isSha256(preview.fabricationOutputSha256)) {
    errors.push("fabrication output hash must be a SHA-256 digest");
  }
  if (!Number.isInteger(preview.pixelPitchUm) || preview.pixelPitchUm <= 0) {
    errors.push("pixel pitch must be a positive integer");
  }
  if (preview.outline.widthUm <= 0 || preview.outline.heightUm <= 0) {
    errors.push("board width and height must be positive");
  }
  const expectedWidthPx =
    preview.pixelPitchUm > 0
      ? Math.ceil(preview.outline.widthUm / preview.pixelPitchUm)
      : 0;
  const expectedHeightPx =
    preview.pixelPitchUm > 0
      ? Math.ceil(preview.outline.heightUm / preview.pixelPitchUm)
      : 0;
  for (const [side, layer] of expected) {
    const matches = preview.textures.filter(
      (texture) => texture.side === side && texture.layer === layer,
    );
    if (matches.length !== 1) {
      errors.push(`${side}.${layer} must have exactly one texture`);
      continue;
    }
    const texture = matches[0];
    if (
      !Number.isInteger(texture.widthPx) ||
      !Number.isInteger(texture.heightPx) ||
      texture.widthPx <= 0 ||
      texture.heightPx <= 0 ||
      texture.rgba.length !== texture.widthPx * texture.heightPx * 4
    ) {
      errors.push(`${side}.${layer} has invalid RGBA dimensions`);
    }
    if (
      texture.widthPx !== expectedWidthPx ||
      texture.heightPx !== expectedHeightPx
    ) {
      errors.push(
        `${side}.${layer} dimensions do not match the compiled production grid`,
      );
    }
  }
  return errors;
}

const PRODUCTION_LAYER_ORDER: readonly ProductionLayer[] = [
  "copper",
  "solderMaskOpen",
  "silkscreen",
];

const DEFAULT_VISIBILITY: ProductionLayerVisibility = {
  copper: true,
  solderMaskOpen: true,
  silkscreen: true,
};

export function buildProductionRenderLayers(
  preview: ProductionPreviewInput,
  selection: ProductionLayerSelection,
): ProductionRenderLayer[] {
  const errors = validateProductionPreview(preview);
  if (errors.length > 0) {
    throw new Error(
      `invalid compiled production preview: ${errors.join("; ")}`,
    );
  }
  const visibility = {
    ...DEFAULT_VISIBILITY,
    ...selection.visibility,
  };
  const visibleLayers = selection.isolatedLayer
    ? [selection.isolatedLayer]
    : PRODUCTION_LAYER_ORDER.filter((layer) => visibility[layer]);
  const viewTransform = productionLayerViewTransform(
    preview,
    selection.side,
    selection.mirroredForViewing ?? selection.side === "back",
  );

  return visibleLayers.map((layer) => ({
    side: selection.side,
    layer,
    texture: selectProductionTexture(preview, selection.side, layer)!,
    viewTransform,
  }));
}

export function productionLayerViewTransform(
  preview: ProductionPreviewInput,
  side: ProductionSide,
  mirroredForViewing: boolean,
): ProductionLayerViewTransform {
  return side === "back" && mirroredForViewing
    ? { scaleX: -1, xUm: preview.outline.widthUm }
    : { scaleX: 1, xUm: 0 };
}

function isSha256(value: string) {
  return /^[0-9a-f]{64}$/i.test(value);
}
