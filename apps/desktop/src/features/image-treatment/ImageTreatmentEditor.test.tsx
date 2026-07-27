import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { ImageTreatmentEditor } from "@/features/image-treatment/ImageTreatmentEditor";
import type {
  AssetReference,
  ImageTreatment,
  TreatmentCompileReport,
} from "@/lib/core";

const asset: AssetReference = {
  id: "asset-1",
  embeddedPath: "media/source.png",
  originalFilename: "portrait.png",
  mediaType: "image/png",
  sha256: "a".repeat(64),
  pixelWidth: 1200,
  pixelHeight: 800,
  folderPath: null,
  tags: [],
  hasAlpha: true,
};

const treatment: ImageTreatment = {
  id: "treatment-1",
  assetId: asset.id,
  productionMode: "monochromeMask",
  recipe: {
    algorithmVersion: "atelier-image-treatment-v2",
    alphaMode: "alphaAsCoverage",
    threshold: { mode: "manual", value: 146 },
    invert: false,
    smoothingRadiusUm: 100,
    despeckleRadiusUm: 80,
    removeIslandsBelowUm2: 150_000,
    minimumLineWidthUm: 120,
    thinFeaturePolicy: "thicken",
    minimumGapUm: 100,
    crop: {
      xMillionths: 50_000,
      yMillionths: 100_000,
      widthMillionths: 850_000,
      heightMillionths: 800_000,
    },
  },
};

const report: TreatmentCompileReport = {
  widthPx: 256,
  heightPx: 160,
  appliedThreshold: 146,
  maskSha256: "b".repeat(64),
  previewPngDataUrl: "data:image/png;base64,AA==",
  pixelPitchUm: 250,
  recipeFingerprint: "c".repeat(64),
  revision: 9,
  purpose: "interactiveProxy",
  topology: { islandCount: 3, holeCount: 1 },
  diagnostics: [
    { kind: "removedIsland", areaUm2: 80_000 },
    { kind: "removedIsland", areaUm2: 60_000 },
    {
      kind: "featureBelowMinimumLineWidth",
      minimumUm: 120,
      measuredUm: 80,
    },
    { kind: "gapBelowMinimum", minimumUm: 100 },
  ],
};

describe("ImageTreatmentEditor", () => {
  it("renders original and processed results with the complete physical recipe", () => {
    const markup = renderToStaticMarkup(
      <ImageTreatmentEditor
        asset={asset}
        compileReport={report}
        colorOriginalAvailable
        onCompileAccepted={() => undefined}
        onConfirm={() => undefined}
        onProductionModeChange={() => undefined}
        originalPreviewUrl="blob:original"
        physicalHeightUm={40_000}
        physicalWidthUm={60_000}
        resultPreviewUrl="blob:result"
        treatment={treatment}
      />,
    );

    expect(markup).toContain("原图");
    expect(markup).toContain("处理结果");
    expect(markup).toContain("黑色 = 生产区域");
    expect(markup).toContain('data-preview-kind="production-mask"');
    expect(markup).toContain('data-preview-size="compact"');
    expect(markup).toContain("treatment-preview-viewport");
    expect(markup).toContain('data-fit-mode="contain"');
    expect(markup).toContain("bg-contain");
    expect(markup).toContain("blob:original");
    expect(markup).toContain("blob:result");
    expect(markup).toContain('aria-label="Alpha 处理"');
    expect(markup).toContain('aria-label="生产方式"');
    expect(markup).toContain("单色生产");
    expect(markup).toContain("彩色原图");
    expect(markup).not.toContain('aria-label="阈值模式"');
    expect(markup).toContain('aria-label="阈值 当前值"');
    expect(markup).toContain('aria-label="阈值 快速调节"');
    expect(markup).toContain('aria-label="重新自动估算阈值"');
    expect(markup).toContain('aria-label="反相"');
    expect(markup).toContain('aria-label="平滑半径 mm 当前值"');
    expect(markup).toContain('aria-label="去斑半径 mm 当前值"');
    expect(markup).toContain('aria-label="去除孤岛 mm² 当前值"');
    expect(markup).toContain('aria-label="最小线宽 mm 当前值"');
    expect(markup).toContain('aria-label="细线处理"');
    expect(markup).toContain('aria-label="最小间距 mm 当前值"');
    expect(markup).toContain('aria-label="平滑半径 mm 快速调节"');
    expect(markup).toContain('aria-label="去斑半径 mm 快速调节"');
    expect(markup).toContain('aria-label="去除孤岛 mm² 快速调节"');
    expect(markup).toContain('aria-label="最小线宽 mm 快速调节"');
    expect(markup).toContain('aria-label="最小间距 mm 快速调节"');
    expect(markup).toContain('aria-label="调整裁切"');
    expect(markup).toContain("当前保留");
    expect(markup).toContain('data-layout="treatment-control"');
    expect(markup).toContain("裁切");
    expect(markup).toContain("确认处理并插入");
  });

  it("uses the original color asset as production preview in color silkscreen mode", () => {
    const markup = renderToStaticMarkup(
      <ImageTreatmentEditor
        asset={asset}
        colorOriginalAvailable
        compileReport={report}
        onCompileAccepted={() => undefined}
        onProductionModeChange={() => undefined}
        originalPreviewUrl="blob:original"
        physicalHeightUm={40_000}
        physicalWidthUm={60_000}
        resultPreviewUrl="blob:result"
        treatment={{ ...treatment, productionMode: "colorOriginal" }}
      />,
    );

    expect(markup).toContain("彩色生产预览");
    expect(markup).toContain('data-preview-kind="color-original"');
    expect(markup).toContain("彩色原图将作为丝印生产资料");
    expect(markup).not.toContain("清理与制造约束");
  });

  it("surfaces manufacturing diagnostics and an explicit display-only original toggle", () => {
    const markup = renderToStaticMarkup(
      <ImageTreatmentEditor
        asset={asset}
        compileReport={report}
        onCompileAccepted={() => undefined}
        originalPreviewUrl="blob:original"
        physicalHeightUm={40_000}
        physicalWidthUm={60_000}
        treatment={treatment}
      />,
    );

    expect(markup).toContain("已移除 2 个小于规则的孤岛");
    expect(markup).toContain("检测到 0.080 mm 线宽，低于 0.120 mm");
    expect(markup).toContain("存在小于 0.100 mm 的间距");
    expect(markup).not.toContain("revision");
    expect(markup).not.toContain("cccccccc");
    expect(markup).toContain('aria-label="临时查看原图"');
    expect(markup).toContain("只切换显示，不修改配方、变换或导出");
  });
});
