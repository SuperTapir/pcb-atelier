import { describe, expect, it } from "vitest";

import {
  buildProductionRenderLayers,
  productionLayerViewTransform,
  selectProductionTexture,
  validateProductionPreview,
  type ProductionPreviewInput,
} from "@/features/preview/production-renderer";
import {
  KonvaProductionRenderer,
  konvaProductionRenderer,
} from "@/features/preview/KonvaProductionRenderer";

const layers = ["copper", "solderMaskOpen", "silkscreen"] as const;

function fixture(): ProductionPreviewInput {
  return {
    source: "resolvedFabricationBoard",
    outline: {
      type: "rectangle",
      widthUm: 200,
      heightUm: 100,
    },
    fabricationInputSha256: "a".repeat(64),
    fabricationOutputSha256: "b".repeat(64),
    pixelPitchUm: 100,
    textures: (["front", "back"] as const).flatMap((side) =>
      layers.map((layer, index) => ({
        side,
        layer,
        widthPx: 2,
        heightPx: 1,
        rgba: [index, 0, 0, 255, 0, 0, 0, 0],
      })),
    ),
  };
}

describe("ProductionRenderer input", () => {
  it("requires exactly the six physical production textures", () => {
    expect(validateProductionPreview(fixture())).toEqual([]);
    const missing = fixture();
    missing.textures.pop();
    expect(validateProductionPreview(missing)).toContain(
      "back.silkscreen must have exactly one texture",
    );

    const wrongGrid = fixture();
    wrongGrid.textures[0].widthPx = 3;
    wrongGrid.textures[0].rgba.push(0, 0, 0, 0);
    expect(validateProductionPreview(wrongGrid)).toContain(
      "front.copper dimensions do not match the compiled production grid",
    );
  });

  it("rejects data that is not identified as a compiled board preview", () => {
    const inferred = fixture();
    inferred.source = "editorContent" as "resolvedFabricationBoard";
    inferred.fabricationOutputSha256 = "";

    expect(validateProductionPreview(inferred)).toEqual([
      "preview source must be ResolvedFabricationBoard",
      "fabrication output hash must be a SHA-256 digest",
    ]);
    expect(() =>
      buildProductionRenderLayers(inferred, {
        side: "front",
      }),
    ).toThrow("invalid compiled production preview");
  });

  it("selects by physical face without applying a data mirror", () => {
    const preview = fixture();
    const texture = selectProductionTexture(
      preview,
      "back",
      "solderMaskOpen",
    );
    expect(texture?.side).toBe("back");
    expect(texture?.rgba).toEqual([1, 0, 0, 255, 0, 0, 0, 0]);
  });

  it("derives visibility and isolation without mutating compiled textures", () => {
    const preview = fixture();
    const original = structuredClone(preview);

    const visible = buildProductionRenderLayers(preview, {
      side: "front",
      visibility: {
        copper: true,
        solderMaskOpen: false,
        silkscreen: true,
      },
    });
    expect(visible.map(({ layer }) => layer)).toEqual([
      "copper",
      "silkscreen",
    ]);

    const isolated = buildProductionRenderLayers(preview, {
      side: "front",
      visibility: {
        copper: true,
        solderMaskOpen: true,
        silkscreen: true,
      },
      isolatedLayer: "solderMaskOpen",
    });
    expect(isolated.map(({ layer }) => layer)).toEqual(["solderMaskOpen"]);
    expect(preview).toEqual(original);
  });

  it("mirrors the back view with a presentation transform only", () => {
    const preview = fixture();
    const originalBack = structuredClone(
      selectProductionTexture(preview, "back", "copper"),
    );
    const renderLayers = buildProductionRenderLayers(preview, {
      side: "back",
      mirroredForViewing: true,
    });

    expect(productionLayerViewTransform(preview, "back", true)).toEqual({
      scaleX: -1,
      xUm: 200,
    });
    expect(renderLayers[0].viewTransform).toEqual({
      scaleX: -1,
      xUm: 200,
    });
    expect(selectProductionTexture(preview, "back", "copper")).toEqual(
      originalBack,
    );
  });

  it("exposes the Konva texture implementation through the replaceable renderer contract", () => {
    expect(konvaProductionRenderer.id).toBe("konva-production-textures");
    expect(konvaProductionRenderer.Component).toBe(KonvaProductionRenderer);
  });
});
