import { useMemo } from "react";
import { Group, Image as KonvaImage } from "react-konva";

import {
  buildProductionRenderLayers,
  type ProductionLayerTexture,
  type ProductionRenderer,
  type ProductionRendererProps,
} from "@/features/preview/production-renderer";

/**
 * Read-only Konva texture stack for the editing canvas.
 *
 * The component deliberately receives only compiled production-preview DTOs.
 * It does not know about content layers, mappings, assets, or rasterization.
 * Its coordinates are millimetres so it can be mounted directly inside the
 * physical board group used by WorkspaceCanvas.
 */
export function KonvaProductionRenderer({
  preview,
  selection,
}: ProductionRendererProps) {
  const renderLayers = buildProductionRenderLayers(preview, selection);
  const widthMm = preview.outline.widthUm / 1_000;
  const heightMm = preview.outline.heightUm / 1_000;
  const viewTransform = renderLayers[0]?.viewTransform;

  return (
    <Group
      listening={false}
      name="compiled-production-textures"
      scaleX={viewTransform?.scaleX ?? 1}
      x={(viewTransform?.xUm ?? 0) / 1_000}
    >
      {renderLayers.map(({ layer, texture }) => (
        <ProductionTexture
          heightMm={heightMm}
          key={`${texture.side}.${layer}`}
          texture={texture}
          widthMm={widthMm}
        />
      ))}
    </Group>
  );
}

function ProductionTexture({
  heightMm,
  texture,
  widthMm,
}: {
  heightMm: number;
  texture: ProductionLayerTexture;
  widthMm: number;
}) {
  const image = useMemo(() => textureCanvas(texture), [texture]);
  if (!image) return null;
  return (
    <KonvaImage
      height={heightMm}
      image={image}
      listening={false}
      name={`production-texture-${texture.side}-${texture.layer}`}
      width={widthMm}
    />
  );
}

function textureCanvas(
  texture: ProductionLayerTexture,
): HTMLCanvasElement | undefined {
  if (typeof document === "undefined") return undefined;
  const canvas = document.createElement("canvas");
  canvas.width = texture.widthPx;
  canvas.height = texture.heightPx;
  const context = canvas.getContext("2d");
  if (!context) return undefined;
  context.putImageData(
    new ImageData(
      Uint8ClampedArray.from(texture.rgba),
      texture.widthPx,
      texture.heightPx,
    ),
    0,
    0,
  );
  return canvas;
}

export const konvaProductionRenderer: ProductionRenderer = {
  id: "konva-production-textures",
  Component: KonvaProductionRenderer,
};
