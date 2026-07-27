import { describe, expect, it } from "vitest";

import {
  cycleOverlappingSelection,
  resolveLayerSelection,
  resolveTreeLayerSelection,
} from "@/features/workspace/layer-selection";
import type { ContentLayer } from "@/lib/core";

const layers = [
  layer("group", null, "group"),
  layer("child", "group", "text"),
  layer("peer", null, "text"),
];

describe("layer selection", () => {
  it("selects a parent group first and drills into the child explicitly", () => {
    expect(
      resolveLayerSelection({
        current: [],
        drillIntoGroup: false,
        layerId: "child",
        layers,
        shiftKey: false,
      }),
    ).toEqual(["group"]);
    expect(
      resolveLayerSelection({
        current: ["group"],
        drillIntoGroup: true,
        layerId: "child",
        layers,
        shiftKey: false,
      }),
    ).toEqual(["child"]);
  });

  it("toggles members with Shift and cycles overlap candidates with Alt", () => {
    expect(
      resolveLayerSelection({
        current: ["peer"],
        drillIntoGroup: false,
        layerId: "group",
        layers,
        shiftKey: true,
      }),
    ).toEqual(["peer", "group"]);
    expect(cycleOverlappingSelection(["peer", "group"], ["peer"])).toEqual([
      "group",
    ]);
  });

  it("supports exact row selection, command toggling, and shift ranges in the tree", () => {
    expect(
      resolveTreeLayerSelection({
        anchorId: null,
        current: [],
        layerId: "child",
        orderedLayerIds: ["group", "child", "peer"],
        rangeKey: false,
        toggleKey: false,
      }),
    ).toEqual({ anchorId: "child", selectedIds: ["child"] });
    expect(
      resolveTreeLayerSelection({
        anchorId: "child",
        current: ["child"],
        layerId: "peer",
        orderedLayerIds: ["group", "child", "peer"],
        rangeKey: false,
        toggleKey: true,
      }),
    ).toEqual({
      anchorId: "peer",
      selectedIds: ["child", "peer"],
    });
    expect(
      resolveTreeLayerSelection({
        anchorId: "group",
        current: ["group"],
        layerId: "peer",
        orderedLayerIds: ["group", "child", "peer"],
        rangeKey: true,
        toggleKey: false,
      }),
    ).toEqual({
      anchorId: "group",
      selectedIds: ["group", "child", "peer"],
    });
  });
});

function layer(
  id: string,
  parentId: string | null,
  type: "group" | "text",
): ContentLayer {
  return {
    id,
    name: id,
    visible: true,
    locked: false,
    exportEnabled: true,
    parentId,
    transform: {
      xUm: 0,
      yUm: 0,
      widthUm: 10_000,
      heightUm: 10_000,
      rotationMdeg: 0,
      flipX: false,
      flipY: false,
    },
    kind:
      type === "group"
        ? { type: "group" }
        : {
            type: "text",
            text: id,
            fontFamily: "sans-serif",
            fontSizeUm: 4_000,
            layout: "autoWidth",
          },
  };
}
