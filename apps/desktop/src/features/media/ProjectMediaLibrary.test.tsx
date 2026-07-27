import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import {
  DEFAULT_PROJECT_MEDIA_DOCK_STATE,
  MediaPreviewDialog,
  ProjectMediaDock,
  ProjectMediaLibrary,
  deriveProjectMediaItems,
  handleProjectMediaExternalDragOver,
  handleProjectMediaExternalDrop,
  loadProjectMediaDockState,
  parseProjectAssetDragPayload,
  readProjectAssetDragPayload,
  saveProjectMediaDockState,
  serializeProjectAssetTextPayload,
  validateProjectMediaFolderPath,
  validateMediaPlacement,
  type ProjectMediaExternalDropEvent,
} from "@/features/media/ProjectMediaLibrary";
import type {
  AssetReference,
  ContentLayer,
  ImageTreatment,
} from "@/lib/core";

const assets: AssetReference[] = [
  {
    id: "asset-logo",
    embeddedPath: "assets/logo.png",
    originalFilename: "Logo.png",
    mediaType: "image/png",
    sha256: "logo-hash",
    pixelWidth: 1200,
    pixelHeight: 800,
    folderPath: "品牌/Logo",
    tags: ["brand"],
    hasAlpha: true,
  },
  {
    id: "asset-photo",
    embeddedPath: "assets/photo.jpg",
    originalFilename: "Front photo.jpg",
    mediaType: "image/jpeg",
    sha256: "photo-hash",
    pixelWidth: 2048,
    pixelHeight: 1536,
    folderPath: null,
    tags: ["front"],
    hasAlpha: false,
  },
];

const treatments: ImageTreatment[] = [
  treatment("treatment-1", "asset-logo", "mask-v1"),
  treatment("treatment-2", "asset-logo", "mask-v2"),
];

const layers: Record<"front" | "back", ContentLayer[]> = {
  front: [imageLayer("front-logo", "asset-logo")],
  back: [
    imageLayer("back-logo", "asset-logo"),
    imageLayer("back-photo", "asset-photo"),
  ],
};

describe("project media dock state", () => {
  it("uses a stable expanded two-panel default and repairs malformed storage", () => {
    expect(DEFAULT_PROJECT_MEDIA_DOCK_STATE).toMatchObject({
      splitPercent: 56,
      productionCollapsed: false,
      mediaCollapsed: false,
      productionScrollTop: 0,
      mediaScrollTop: 0,
      expandedMediaFolders: ["*"],
    });

    const storage = memoryStorage(
      JSON.stringify({
        splitPercent: 99,
        productionCollapsed: "no",
        mediaCollapsed: true,
        productionScrollTop: -8,
        mediaScrollTop: 120,
        expandedProductionNodes: ["front", 3],
        expandedMediaFolders: ["品牌", "品牌", 2],
        selectedAssetId: 42,
        mediaViewMode: "table",
      }),
    );

    expect(loadProjectMediaDockState(storage)).toEqual({
      ...DEFAULT_PROJECT_MEDIA_DOCK_STATE,
      splitPercent: 75,
      mediaCollapsed: true,
      mediaScrollTop: 120,
      expandedProductionNodes: ["front"],
      expandedMediaFolders: ["品牌"],
    });
  });

  it("round-trips device-local panel, scroll and expansion preferences", () => {
    const storage = memoryStorage(null);
    const expected = {
      ...DEFAULT_PROJECT_MEDIA_DOCK_STATE,
      splitPercent: 42,
      productionScrollTop: 84,
      mediaScrollTop: 160,
      expandedProductionNodes: ["front", "front/copper"],
      expandedMediaFolders: ["品牌/Logo"],
      selectedAssetId: "asset-logo",
      mediaViewMode: "list" as const,
    };

    saveProjectMediaDockState(expected, storage);

    expect(loadProjectMediaDockState(storage)).toEqual(expected);
  });
});

describe("project media derivation and placement", () => {
  it("normalizes folder segments and rejects traversal or absolute paths", () => {
    expect(validateProjectMediaFolderPath(" 品牌 / 标志 ")).toEqual({
      allowed: true,
      folderPath: "品牌/标志",
    });
    expect(validateProjectMediaFolderPath("  ")).toEqual({
      allowed: true,
      folderPath: null,
    });
    expect(validateProjectMediaFolderPath("../外部")).toMatchObject({
      allowed: false,
    });
    expect(validateProjectMediaFolderPath("/绝对路径")).toMatchObject({
      allowed: false,
    });
  });

  it("searches filename, folder and tags while deriving usage and versions", () => {
    const [logo] = deriveProjectMediaItems(
      assets,
      layers,
      treatments,
      "brand",
    );

    expect(logo).toMatchObject({
      id: "asset-logo",
      usageCount: 2,
      treatmentCount: 2,
      algorithmVersions: ["mask-v1", "mask-v2"],
    });
    expect(
      deriveProjectMediaItems(assets, layers, treatments, "品牌/Logo"),
    ).toHaveLength(1);
    expect(
      deriveProjectMediaItems(assets, layers, treatments, "missing"),
    ).toEqual([]);
  });

  it("allows valid images on the active production layer and rejects invalid media", () => {
    expect(
      validateMediaPlacement(assets[0], {
        face: "back",
        productionLayer: "solderMaskOpen",
      }),
    ).toEqual({ allowed: true });
    expect(
      validateMediaPlacement(
        { ...assets[0], mediaType: "application/pdf" },
        { face: "front", productionLayer: "copper" },
      ),
    ).toEqual({
      allowed: false,
      reason: "当前素材不是可放置的图片内容",
    });
  });

  it("parses only the project-asset drag payload contract", () => {
    expect(parseProjectAssetDragPayload('{"assetId":"asset-logo"}')).toEqual({
      assetId: "asset-logo",
    });
    expect(parseProjectAssetDragPayload('{"assetId":42}')).toBeNull();
    expect(parseProjectAssetDragPayload("not-json")).toBeNull();
  });

  it("reads a namespaced text fallback when WebView strips custom drag MIME types", () => {
    const payload = serializeProjectAssetTextPayload("asset-logo");
    expect(
      readProjectAssetDragPayload({
        types: ["text/plain"],
        getData: (type) => (type === "text/plain" ? payload : ""),
      }),
    ).toEqual({ assetId: "asset-logo" });
    expect(
      readProjectAssetDragPayload({
        types: ["text/plain"],
        getData: () => "ordinary dragged text",
      }),
    ).toBeNull();
  });

  it("imports only external images into the library and stops global insertion", () => {
    const preventDefault = vi.fn();
    const stopPropagation = vi.fn();
    const event: ProjectMediaExternalDropEvent = {
      dataTransfer: {
        files: [
          { name: "logo.png", type: "image/png" },
          { name: "notes.txt", type: "text/plain" },
        ] as File[],
        types: ["Files"],
        dropEffect: "none",
      },
      preventDefault,
      stopPropagation,
    };
    const onImportFiles = vi.fn();
    const onInvalid = vi.fn();

    expect(handleProjectMediaExternalDragOver(event)).toBe(true);
    expect(event.dataTransfer.dropEffect).toBe("copy");
    expect(
      handleProjectMediaExternalDrop(event, onImportFiles, onInvalid),
    ).toBe(true);

    expect(preventDefault).toHaveBeenCalledTimes(2);
    expect(stopPropagation).toHaveBeenCalledTimes(2);
    expect(onImportFiles).toHaveBeenCalledWith([
      expect.objectContaining({ name: "logo.png" }),
    ]);
    expect(onInvalid).toHaveBeenCalledWith("仅支持 PNG、JPEG 或 WebP 图片");
  });

  it("does not consume internal project-asset drags as external files", () => {
    const event: ProjectMediaExternalDropEvent = {
      dataTransfer: {
        files: [] as File[],
        types: ["application/x-pcb-atelier-project-asset"],
        dropEffect: "none",
      },
      preventDefault: vi.fn(),
      stopPropagation: vi.fn(),
    };

    expect(handleProjectMediaExternalDragOver(event)).toBe(false);
    expect(
      handleProjectMediaExternalDrop(event, vi.fn(), vi.fn()),
    ).toBe(false);
    expect(event.preventDefault).not.toHaveBeenCalled();
    expect(event.stopPropagation).not.toHaveBeenCalled();
  });
});

describe("ProjectMediaLibrary", () => {
  it("flattens a lone uncategorized folder but keeps real folder hierarchy", () => {
    const flatMarkup = renderToStaticMarkup(
      <ProjectMediaLibrary
        activeFace="front"
        activeProductionLayer="silkscreen"
        assets={[assets[1]]}
        initialState={DEFAULT_PROJECT_MEDIA_DOCK_STATE}
        layers={{ front: [], back: [] }}
        onPlaceAsset={() => undefined}
        thumbnailUrls={{}}
        treatments={[]}
      />,
    );
    expect(flatMarkup).toContain("Front photo.jpg");
    expect(flatMarkup).not.toContain("未分类");

    const groupedMarkup = renderToStaticMarkup(
      <ProjectMediaLibrary
        activeFace="front"
        activeProductionLayer="silkscreen"
        assets={assets}
        initialState={DEFAULT_PROJECT_MEDIA_DOCK_STATE}
        layers={{ front: [], back: [] }}
        onPlaceAsset={() => undefined}
        thumbnailUrls={{}}
        treatments={treatments}
      />,
    );
    expect(groupedMarkup).toContain("品牌/Logo");
    expect(groupedMarkup).toContain("未分类");
  });

  it("keeps media cards focused on thumbnail, filename and usage", () => {
    const markup = renderToStaticMarkup(
      <ProjectMediaLibrary
        activeFace="front"
        activeProductionLayer="silkscreen"
        assets={assets}
        initialState={DEFAULT_PROJECT_MEDIA_DOCK_STATE}
        layers={layers}
        onPlaceAsset={() => undefined}
        thumbnailUrls={{ "asset-logo": "data:image/png;base64,AA==" }}
        treatments={treatments}
      />,
    );

    expect(markup).toContain("项目媒体");
    expect(markup).toContain("品牌/Logo");
    expect(markup).toContain("使用 2 次");
    expect(markup).not.toContain("1200 × 800");
    expect(markup).not.toContain("含透明通道");
    expect(markup).not.toContain("2 个处理版本");
    expect(markup).not.toContain("mask-v1");
    expect(markup).not.toContain(">放置<");
    expect(markup).toContain('draggable="true"');
    expect(markup).toContain(
      'aria-label="Logo.png，点击预览，可拖到正面丝印层"',
    );
  });

  it("shows a large accessible preview with metadata and an explicit placement action", () => {
    const markup = renderToStaticMarkup(
      <MediaPreviewDialog
        item={{
          ...assets[0],
          usageCount: 2,
          treatmentCount: 2,
          algorithmVersions: ["mask-v1", "mask-v2"],
        }}
        onClose={() => undefined}
        onPlace={() => undefined}
        target={{ face: "front", productionLayer: "silkscreen" }}
        thumbnailUrl="data:image/png;base64,AA=="
      />,
    );

    expect(markup).toContain('role="dialog"');
    expect(markup).toContain('aria-label="预览 Logo.png"');
    expect(markup).toContain('alt="Logo.png"');
    expect(markup).toContain("1200 × 800");
    expect(markup).toContain("放置到正面丝印层");
  });

  it("shows full metadata only for the selected asset", () => {
    const markup = renderToStaticMarkup(
      <ProjectMediaLibrary
        activeFace="front"
        activeProductionLayer="silkscreen"
        assets={assets}
        initialState={{
          ...DEFAULT_PROJECT_MEDIA_DOCK_STATE,
          selectedAssetId: "asset-logo",
        }}
        layers={layers}
        onPlaceAsset={() => undefined}
        thumbnailUrls={{ "asset-logo": "data:image/png;base64,AA==" }}
        treatments={treatments}
      />,
    );

    expect(markup).toContain('aria-label="素材详情"');
    expect(markup).toContain("1200 × 800");
    expect(markup).toContain("PNG");
    expect(markup).toContain("含透明通道");
    expect(markup).toContain("2 个处理版本");
    expect(markup).toContain("mask-v1");
    expect(markup).not.toContain("ring-1 ring-primary");
    expect(markup).toContain("min-w-0 max-w-full overflow-hidden");
  });

  it("offers an explicit folder path editor for the selected asset", () => {
    const markup = renderToStaticMarkup(
      <ProjectMediaLibrary
        activeFace="front"
        activeProductionLayer="silkscreen"
        assets={assets}
        initialState={{
          ...DEFAULT_PROJECT_MEDIA_DOCK_STATE,
          selectedAssetId: "asset-photo",
        }}
        layers={{ front: [], back: [] }}
        onMoveAsset={() => undefined}
        onPlaceAsset={() => undefined}
        thumbnailUrls={{}}
        treatments={treatments}
      />,
    );

    expect(markup).toContain('aria-label="素材文件夹路径"');
    expect(markup).toContain('placeholder="例如 品牌/Logo"');
    expect(markup).toContain(">移动素材<");
    expect(markup).toContain('value="品牌/Logo"');
    expect(markup).toContain("留空可移至未分类");
    expect(markup).toContain("未使用");
  });

  it("keeps the production tree and media library visible in the default dock", () => {
    const markup = renderToStaticMarkup(
      <ProjectMediaDock
        activeFace="front"
        activeProductionLayer="copper"
        assets={assets}
        initialState={DEFAULT_PROJECT_MEDIA_DOCK_STATE}
        layers={layers}
        onPlaceAsset={() => undefined}
        productionPanel={<div>生产层树</div>}
        treatments={treatments}
      />,
    );

    expect(markup).toContain("生产层树");
    expect(markup).toContain("项目媒体");
    expect(markup).toContain('role="separator"');
    expect(markup).toContain('aria-orientation="horizontal"');
  });

  it("renders list mode and disables insertion for invalid project content", () => {
    const markup = renderToStaticMarkup(
      <ProjectMediaLibrary
        activeFace="back"
        activeProductionLayer="copper"
        assets={[{ ...assets[0], mediaType: "application/pdf" }]}
        initialState={{
          ...DEFAULT_PROJECT_MEDIA_DOCK_STATE,
          mediaViewMode: "list",
        }}
        layers={{ front: [], back: [] }}
        onPlaceAsset={() => undefined}
        thumbnailUrls={{}}
        treatments={[]}
      />,
    );

    expect(markup).toContain('aria-label="列表视图"');
    expect(markup).toContain('aria-pressed="true"');
    expect(markup).toContain('draggable="false"');
    expect(markup).toContain("当前素材不是可放置的图片内容");
    expect(markup).toContain('aria-disabled="true"');
  });
});

function treatment(
  id: string,
  assetId: string,
  algorithmVersion: string,
): ImageTreatment {
  return {
    id,
    assetId,
    productionMode: "monochromeMask",
    recipe: {
      algorithmVersion,
      alphaMode: "alphaAsCoverage",
      threshold: { mode: "otsu" },
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
}

function imageLayer(id: string, assetId: string): ContentLayer {
  return {
    id,
    name: id,
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
    kind: { type: "image", assetId, crop: null },
  };
}

function memoryStorage(initial: string | null) {
  let value = initial;
  return {
    getItem: () => value,
    setItem: (_key: string, next: string) => {
      value = next;
    },
  };
}
