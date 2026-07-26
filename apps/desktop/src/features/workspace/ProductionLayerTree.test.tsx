import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { ProductionLayerTree } from "@/features/workspace/ProductionLayerTree";
import type { ContentLayer, ProductionMapping } from "@/lib/core";

const layer: ContentLayer = {
  id: "source-1",
  name: "Logo",
  visible: true,
  locked: false,
  exportEnabled: true,
  parentId: null,
  transform: {
    xUm: 0,
    yUm: 0,
    widthUm: 10_000,
    heightUm: 10_000,
    rotationMdeg: 0,
    flipX: false,
    flipY: false,
  },
  kind: {
    type: "text",
    text: "Logo",
    fontFamily: "sans-serif",
    fontSizeUm: 3_000,
    layout: "autoWidth",
  },
};

const mappings: ProductionMapping[] = [
  {
    id: "mapping-1",
    sourceLayerId: layer.id,
    target: { side: "front", layer: "copper" },
    combine: "add",
  },
  {
    id: "mapping-2",
    sourceLayerId: layer.id,
    target: { side: "front", layer: "solderMaskOpen" },
    combine: "add",
  },
];

const baseProps = {
  activeFace: "front" as const,
  boardSelected: false,
  contexts: { front: "copper" as const, back: "silkscreen" as const },
  layers: { front: [layer], back: [] },
  mappings,
  selectedIds: { front: [], back: [] },
  onCreateBoardFill: () => undefined,
  onSelectBoard: () => undefined,
  onSelectContext: () => undefined,
  onSelectSource: () => undefined,
};

describe("ProductionLayerTree", () => {
  it("shows the fixed board/front/back hierarchy without a content entry", () => {
    const markup = renderToStaticMarkup(<ProductionLayerTree {...baseProps} />);

    expect(markup).toContain("板体");
    expect(markup).toContain("正面");
    expect(markup).toContain("背面");
    for (const face of ["front", "back"]) {
      for (const context of ["copper", "solderMaskOpen", "silkscreen"]) {
        expect(markup).toContain(
          `data-testid="production-context-${face}-${context}"`,
        );
      }
    }
    expect(markup).not.toContain(">内容<");
  });

  it("shows objects directly under their production layer and marks shared sources", () => {
    const markup = renderToStaticMarkup(<ProductionLayerTree {...baseProps} />);

    expect(markup).toContain("Logo");
    expect(markup).toContain("关联");
    expect(markup).toContain("同一源对象 source-1");
  });

  it("keeps production-layer expansion separate from the focused work layer", () => {
    const markup = renderToStaticMarkup(<ProductionLayerTree {...baseProps} />);

    expect(markup).toContain('aria-label="收起正面铜层"');
    expect(markup).toContain('aria-label="展开正面阻焊开窗"');
    expect(markup).toContain('aria-label="展开正面丝印层"');
  });

  it("offers board fill only inside an active copper container", () => {
    const copperMarkup = renderToStaticMarkup(
      <ProductionLayerTree {...baseProps} />,
    );
    const silkMarkup = renderToStaticMarkup(
      <ProductionLayerTree
        {...baseProps}
        contexts={{ front: "silkscreen", back: "silkscreen" }}
      />,
    );

    expect(copperMarkup).toContain("添加基础铺铜");
    expect(silkMarkup).not.toContain("添加基础铺铜");
  });
});
