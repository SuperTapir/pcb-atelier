import { describe, expect, it } from "vitest";

import {
  getFitViewport,
  classifySecondaryPointerGesture,
  displayedXToBoardX,
  getBoardDropPoint,
  getWheelZoom,
  getMarqueeLayerIds,
  getAdaptiveProxyPixelPitchUm,
  getInteractiveProxyCacheKey,
  getTranslatedSelectionTransforms,
  isVisibleInProductionContext,
  mergeAvailableProxyImages,
  resolveTransformerKeepRatio,
  shouldClearCanvasSelection,
} from "@/features/workspace/WorkspaceCanvas";
import type {
  ContentLayer,
  ImageTreatment,
  ProductionMapping,
} from "@/lib/core";

describe("back-face view transform", () => {
  it("keeps both editing faces in upright physical coordinates", () => {
    expect(displayedXToBoardX(12, "front", 64)).toBe(12);
    expect(displayedXToBoardX(12, "back", 64)).toBe(12);
  });

  it("does not rewrite back-face domain coordinates for presentation", () => {
    const physicalX = 17;
    const displayedX = displayedXToBoardX(physicalX, "back", 64);
    expect(displayedX).toBe(physicalX);
  });
});

describe("canvas blank selection", () => {
  it("clears selection only on an already-active canvas with the select tool", () => {
    expect(shouldClearCanvasSelection("select", true, true)).toBe(true);
    expect(shouldClearCanvasSelection("select", true, false)).toBe(false);
    expect(shouldClearCanvasSelection("text", true, true)).toBe(false);
    expect(shouldClearCanvasSelection("select", false, true)).toBe(false);
  });
});

describe("canvas pointer gestures", () => {
  it("converts a client drop position to the physical board point in micrometres", () => {
    expect(
      getBoardDropPoint({
        boardHeightMm: 100,
        boardWidthMm: 64,
        bounds: { height: 600, left: 100, top: 50, width: 500 },
        clientX: 339,
        clientY: 377.5,
        face: "back",
        viewport: { panX: 0, panY: 0, zoom: 1 },
      }),
    ).toEqual({ xUm: 30_000, yUm: 55_000 });
  });

  it("rejects a drop in the canvas chrome outside the physical board", () => {
    expect(
      getBoardDropPoint({
        boardHeightMm: 100,
        boardWidthMm: 64,
        bounds: { height: 600, left: 100, top: 50, width: 500 },
        clientX: 110,
        clientY: 60,
        face: "front",
        viewport: { panX: 0, panY: 0, zoom: 1 },
      }),
    ).toBeNull();
  });

  it("applies a damped continuous wheel zoom step", () => {
    expect(getWheelZoom(1, -100, "medium")).toBeCloseTo(1.073, 2);
    expect(getWheelZoom(1, 100, "medium")).toBeCloseTo(0.932, 2);
    expect(getWheelZoom(1, -100, "high")).toBeLessThan(
      getWheelZoom(1, -100, "medium"),
    );
    expect(getWheelZoom(1, -100, "low")).toBeGreaterThan(
      getWheelZoom(1, -100, "medium"),
    );
  });

  it("uses a movement threshold to separate a right click from viewport pan", () => {
    expect(
      classifySecondaryPointerGesture({ x: 10, y: 10 }, { x: 12, y: 13 }),
    ).toBe("context-menu");
    expect(
      classifySecondaryPointerGesture({ x: 10, y: 10 }, { x: 18, y: 10 }),
    ).toBe("pan");
  });

  it("marquee-selects intersecting unlocked layers without changing geometry", () => {
    const layers = [
      {
        id: "inside",
        kind: { type: "text" },
        parentId: null,
        visible: true,
        locked: false,
        transform: { xUm: 10_000, yUm: 10_000, widthUm: 5_000, heightUm: 5_000 },
      },
      {
        id: "locked",
        kind: { type: "text" },
        visible: true,
        locked: true,
        transform: { xUm: 12_000, yUm: 12_000, widthUm: 2_000, heightUm: 2_000 },
      },
      {
        id: "outside",
        kind: { type: "text" },
        visible: true,
        locked: false,
        transform: { xUm: 30_000, yUm: 30_000, widthUm: 2_000, heightUm: 2_000 },
      },
    ] as ContentLayer[];

    expect(
      getMarqueeLayerIds(layers, {
        minXUm: 9_000,
        minYUm: 9_000,
        maxXUm: 20_000,
        maxYUm: 20_000,
      }),
    ).toEqual(["inside"]);
  });

  it("translates every movable selected layer by the same drag delta", () => {
    const layers = [
      {
        id: "first",
        kind: { type: "text" },
        parentId: null,
        visible: true,
        locked: false,
        transform: {
          xUm: 10_000,
          yUm: 20_000,
          widthUm: 5_000,
          heightUm: 6_000,
          rotationMdeg: 0,
          flipX: false,
          flipY: false,
        },
      },
      {
        id: "second",
        kind: { type: "image" },
        parentId: null,
        visible: true,
        locked: false,
        transform: {
          xUm: 30_000,
          yUm: 40_000,
          widthUm: 7_000,
          heightUm: 8_000,
          rotationMdeg: 12_000,
          flipX: true,
          flipY: false,
        },
      },
    ] as ContentLayer[];

    expect(
      getTranslatedSelectionTransforms(layers, ["first", "second"], 2_500, -3_000),
    ).toEqual([
      {
        layerId: "first",
        transform: { ...layers[0].transform, xUm: 12_500, yUm: 17_000 },
      },
      {
        layerId: "second",
        transform: { ...layers[1].transform, xUm: 32_500, yUm: 37_000 },
      },
    ]);
  });
});

describe("canvas aspect-ratio constraint", () => {
  it("lets the shared inspector lock control freeform canvas resizing", () => {
    expect(resolveTransformerKeepRatio(true)).toBe(true);
    expect(resolveTransformerKeepRatio(false)).toBe(false);
  });
});

describe("fit board viewport", () => {
  it("uses available large-screen space instead of capping fit at 100%", () => {
    expect(
      getFitViewport({
        boardHeightMm: 100,
        boardWidthMm: 64,
        canvasHeightPx: 900,
        canvasWidthPx: 900,
      }),
    ).toEqual({ zoom: 1.46, panX: 0, panY: 0 });
  });

  it("shrinks a board to preserve a safe margin in a compact canvas", () => {
    expect(
      getFitViewport({
        boardHeightMm: 100,
        boardWidthMm: 64,
        canvasHeightPx: 360,
        canvasWidthPx: 280,
      }),
    ).toEqual({ zoom: 0.48, panX: 0, panY: 0 });
  });
});

describe("interactive production proxy cache", () => {
  it("keeps the last drawable proxy until a refreshed proxy is ready", () => {
    const previous = new Map([
      ["image-a", "old-a"],
      ["image-b", "old-b"],
    ]);

    expect(
      mergeAvailableProxyImages(previous, ["image-a", "image-b"], [
        ["image-b", "new-b"],
      ]),
    ).toEqual(
      new Map([
        ["image-a", "old-a"],
        ["image-b", "new-b"],
      ]),
    );
  });

  it("drops retained proxies only after their source layer is removed", () => {
    expect(
      mergeAvailableProxyImages(
        new Map([
          ["removed", "old"],
          ["active", "old-active"],
        ]),
        ["active"],
        [],
      ),
    ).toEqual(new Map([["active", "old-active"]]));
  });

  it("increases mask density as the user zooms in", () => {
    expect(getAdaptiveProxyPixelPitchUm(0.5)).toBe(250);
    expect(getAdaptiveProxyPixelPitchUm(1)).toBe(100);
    expect(getAdaptiveProxyPixelPitchUm(2)).toBe(50);
    expect(getAdaptiveProxyPixelPitchUm(4)).toBe(25);
    expect(getAdaptiveProxyPixelPitchUm(16)).toBe(25);
  });

  it("ignores position and manufacturing colour but changes for geometry or recipe", () => {
    const treatment: ImageTreatment = {
      id: "treatment-1",
      assetId: "asset-1",
      productionMode: "monochromeMask",
      recipe: {
        algorithmVersion: "atelier-image-treatment-v2",
        alphaMode: "compositeOnWhite",
        threshold: { mode: "manual", value: 128 },
        invert: false,
        smoothingRadiusUm: 0,
        despeckleRadiusUm: 0,
        removeIslandsBelowUm2: 0,
        minimumLineWidthUm: 0,
        thinFeaturePolicy: "preserve",
        minimumGapUm: 0,
        crop: null,
      },
    };
    expect(getInteractiveProxyCacheKey(treatment, 20_000, 10_000)).toBe(
      getInteractiveProxyCacheKey(treatment, 20_000, 10_000),
    );
    expect(getInteractiveProxyCacheKey(treatment, 20_000, 10_000)).not.toBe(
      getInteractiveProxyCacheKey(
        { ...treatment, recipe: { ...treatment.recipe, invert: true } },
        20_000,
        10_000,
      ),
    );
    expect(getInteractiveProxyCacheKey(treatment, 20_000, 10_000)).not.toBe(
      getInteractiveProxyCacheKey(treatment, 21_000, 10_000),
    );
    expect(
      getInteractiveProxyCacheKey(treatment, 20_000, 10_000, 100),
    ).not.toBe(getInteractiveProxyCacheKey(treatment, 20_000, 10_000, 50));
  });
});

describe("production-layer visibility in the edit canvas", () => {
  const layer = { id: "text-1" } as ContentLayer;
  const mappings: ProductionMapping[] = [
    {
      id: "mapping-1",
      sourceLayerId: layer.id,
      target: { side: "front", layer: "silkscreen" },
      combine: "add",
    },
  ];

  it("renders one editable source object when its production layer is visible", () => {
    expect(
      isVisibleInProductionContext(layer, mappings, "front", {
        copper: false,
        solderMaskOpen: false,
        silkscreen: true,
      }),
    ).toBe(true);
  });

  it("hides the source object with its only mapped production layer", () => {
    expect(
      isVisibleInProductionContext(layer, mappings, "front", {
        copper: true,
        solderMaskOpen: true,
        silkscreen: false,
      }),
    ).toBe(false);
  });

  it("keeps an object visible when any associated production layer is visible", () => {
    const associated: ProductionMapping[] = [
      ...mappings,
      {
        id: "mapping-2",
        sourceLayerId: layer.id,
        target: { side: "front", layer: "solderMaskOpen" },
        combine: "add",
      },
    ];
    expect(
      isVisibleInProductionContext(layer, associated, "front", {
        copper: false,
        solderMaskOpen: true,
        silkscreen: false,
      }),
    ).toBe(true);
  });
});
