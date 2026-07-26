import { describe, expect, it } from "vitest";

import {
  GeometryEditError,
  applyTransformPatch,
  axisAlignedBounds,
  isLayerTransformEditable,
  nudgeTransform,
  parseDegreesToMdeg,
  parseMillimetresToUm,
  snapTransform,
  type GeometryLayer,
} from "@/features/workspace/geometry-edit";
import type { TransformUm } from "@/lib/core";

const BASE_TRANSFORM: TransformUm = {
  xUm: 10_000,
  yUm: 20_000,
  widthUm: 8_000,
  heightUm: 4_000,
  rotationMdeg: 0,
  flipX: false,
  flipY: false,
};

function layer(
  id: string,
  transform: TransformUm = BASE_TRANSFORM,
  options: { locked?: boolean; parentId?: string | null } = {},
): GeometryLayer {
  return {
    id,
    locked: options.locked ?? false,
    parentId: options.parentId ?? null,
    transform: { ...transform },
  };
}

describe("parseMillimetresToUm", () => {
  it("parses decimal millimetres to exact integer micrometres", () => {
    expect(parseMillimetresToUm(" 12.345 ", "position")).toBe(12_345);
    expect(parseMillimetresToUm("-0.125", "position")).toBe(-125);
    expect(parseMillimetresToUm("8.0000", "size")).toBe(8_000);
  });

  it("rejects NaN, negative sizes, zero sizes and sub-micrometre precision", () => {
    expect(() => parseMillimetresToUm("NaN", "position")).toThrowError(
      expect.objectContaining({ code: "invalidNumber" }),
    );
    expect(() => parseMillimetresToUm("-1", "size")).toThrowError(
      expect.objectContaining({ code: "nonPositiveSize" }),
    );
    expect(() => parseMillimetresToUm("0", "size")).toThrowError(
      expect.objectContaining({ code: "nonPositiveSize" }),
    );
    expect(() => parseMillimetresToUm("1.0001", "position")).toThrowError(
      expect.objectContaining({ code: "subMicrometrePrecision" }),
    );
  });
});

describe("parseDegreesToMdeg", () => {
  it("parses signed degrees to exact integer millidegrees", () => {
    expect(parseDegreesToMdeg("-12.375")).toBe(-12_375);
    expect(parseDegreesToMdeg("90")).toBe(90_000);
  });

  it("rejects sub-millidegree precision", () => {
    expect(() => parseDegreesToMdeg("0.0001")).toThrowError(
      expect.objectContaining({ code: "subMilliDegreePrecision" }),
    );
  });
});

describe("applyTransformPatch and nudgeTransform", () => {
  it("applies integer transform fields exactly without changing untouched fields", () => {
    const selected = layer("selected");

    expect(
      applyTransformPatch(selected, [selected], {
        xUm: -250,
        widthUm: 12_345,
        rotationMdeg: 45_125,
        flipX: true,
      }),
    ).toEqual({
      ...BASE_TRANSFORM,
      xUm: -250,
      widthUm: 12_345,
      rotationMdeg: 45_125,
      flipX: true,
    });
  });

  it("nudges by 0.1 mm, or 1 mm while Shift is held", () => {
    const selected = layer("selected");

    expect(nudgeTransform(selected, [selected], "left", false).xUm).toBe(9_900);
    expect(nudgeTransform(selected, [selected], "down", false).yUm).toBe(
      20_100,
    );
    expect(nudgeTransform(selected, [selected], "right", true).xUm).toBe(
      11_000,
    );
    expect(nudgeTransform(selected, [selected], "up", true).yUm).toBe(19_000);
  });

  it("rejects edits when the layer itself or any ancestor is locked", () => {
    const locked = layer("locked", BASE_TRANSFORM, { locked: true });
    expect(() =>
      applyTransformPatch(locked, [locked], { xUm: 30_000 }),
    ).toThrowError(expect.objectContaining({ code: "layerLocked" }));

    const parent = layer("group", BASE_TRANSFORM, { locked: true });
    const child = layer("child", BASE_TRANSFORM, { parentId: parent.id });
    expect(() =>
      nudgeTransform(child, [parent, child], "right", false),
    ).toThrowError(expect.objectContaining({ code: "ancestorLocked" }));
    expect(isLayerTransformEditable(locked, [locked])).toBe(false);
    expect(isLayerTransformEditable(child, [parent, child])).toBe(false);
    expect(isLayerTransformEditable(layer("free"), [layer("free")])).toBe(true);
  });
});

describe("axisAlignedBounds", () => {
  it("computes the axis-aligned bounds of a rotated object around its center", () => {
    expect(
      axisAlignedBounds({
        ...BASE_TRANSFORM,
        widthUm: 10_000,
        heightUm: 4_000,
        rotationMdeg: 90_000,
      }),
    ).toEqual({
      leftUm: 13_000,
      topUm: 17_000,
      rightUm: 17_000,
      bottomUm: 27_000,
      centerXUm: 15_000,
      centerYUm: 22_000,
      widthUm: 4_000,
      heightUm: 10_000,
    });
  });
});

describe("snapTransform", () => {
  it("snaps to grid and emits visible guide descriptions", () => {
    const selected = layer("selected");
    const result = snapTransform({
      layer: selected,
      layers: [selected],
      proposedTransform: { ...BASE_TRANSFORM, xUm: 9_850, yUm: 20_120 },
      board: { widthUm: 64_000, heightUm: 100_000 },
      gridStepUm: 5_000,
      thresholdUm: 200,
      altKey: false,
    });

    expect(result.transform.xUm).toBe(10_000);
    expect(result.transform.yUm).toBe(20_000);
    expect(result.guides).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          axis: "x",
          kind: "grid",
          positionUm: 10_000,
          description: "X 对齐到 10.000 mm 网格",
        }),
        expect.objectContaining({
          axis: "y",
          kind: "grid",
          positionUm: 20_000,
          description: "Y 对齐到 20.000 mm 网格",
        }),
      ]),
    );
  });

  it("snaps rotated bounds to board edges", () => {
    const selected = layer("selected");
    const result = snapTransform({
      layer: selected,
      layers: [selected],
      proposedTransform: {
        ...BASE_TRANSFORM,
        xUm: 57_050,
        yUm: 30_000,
        widthUm: 10_000,
        heightUm: 4_000,
        rotationMdeg: 90_000,
      },
      board: { widthUm: 64_000, heightUm: 100_000 },
      gridStepUm: 0,
      thresholdUm: 100,
      altKey: false,
    });

    expect(axisAlignedBounds(result.transform).rightUm).toBe(64_000);
    expect(result.guides).toContainEqual(
      expect.objectContaining({
        axis: "x",
        kind: "boardEdge",
        positionUm: 64_000,
        description: "右边缘对齐到板右边",
      }),
    );
  });

  it("snaps to other object edges and centers without using hidden objects", () => {
    const selected = layer("selected");
    const edgeTarget = {
      ...layer("edge-target", {
        ...BASE_TRANSFORM,
        xUm: 30_000,
        yUm: 40_000,
        widthUm: 10_000,
        heightUm: 10_000,
      }),
      visible: true,
    };
    const hiddenCloserTarget = {
      ...layer("hidden-target", {
        ...BASE_TRANSFORM,
        xUm: 29_950,
        yUm: 40_000,
        widthUm: 10_000,
        heightUm: 10_000,
      }),
      visible: false,
    };
    const result = snapTransform({
      layer: selected,
      layers: [selected, edgeTarget, hiddenCloserTarget],
      proposedTransform: {
        ...BASE_TRANSFORM,
        xUm: 21_900,
        yUm: 42_900,
        widthUm: 8_000,
        heightUm: 4_000,
      },
      board: { widthUm: 64_000, heightUm: 100_000 },
      gridStepUm: 0,
      thresholdUm: 150,
      altKey: false,
    });

    expect(result.transform.xUm).toBe(22_000);
    expect(result.transform.yUm).toBe(43_000);
    expect(result.guides).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          axis: "x",
          kind: "objectEdge",
          targetLayerId: "edge-target",
          description: "右边缘对齐到「edge-target」左边缘",
        }),
        expect.objectContaining({
          axis: "y",
          kind: "objectCenter",
          targetLayerId: "edge-target",
          description: "垂直中心对齐到「edge-target」垂直中心",
        }),
      ]),
    );
  });

  it("temporarily bypasses snapping while Alt is held", () => {
    const selected = layer("selected");
    const proposed = { ...BASE_TRANSFORM, xUm: 9_850 };
    const result = snapTransform({
      layer: selected,
      layers: [selected],
      proposedTransform: proposed,
      board: { widthUm: 64_000, heightUm: 100_000 },
      gridStepUm: 5_000,
      thresholdUm: 200,
      altKey: true,
    });

    expect(result).toEqual({
      transform: proposed,
      guides: [],
      bypassed: true,
    });
  });

  it("does not let Alt bypass a layer lock", () => {
    const selected = layer("selected", BASE_TRANSFORM, { locked: true });
    expect(() =>
      snapTransform({
        layer: selected,
        layers: [selected],
        proposedTransform: { ...BASE_TRANSFORM, xUm: 9_850 },
        board: { widthUm: 64_000, heightUm: 100_000 },
        gridStepUm: 5_000,
        thresholdUm: 200,
        altKey: true,
      }),
    ).toThrowError(GeometryEditError);
  });
});
