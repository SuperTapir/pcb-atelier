import { expect, test, type Locator, type Page } from "./test";
import fs from "node:fs";
import path from "node:path";

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto("/");
  await expect(
    page.locator("header").getByText("双面非对称黄金卡", { exact: true }),
  ).toBeVisible();
});

test("浏览器完整创作流贯通双面图片处理、素材复用、生产层、3D 与 EasyEDA", async ({
  page,
}, testInfo) => {
  test.setTimeout(90_000);
  const filename = "full-creation-flow-art.png";
  const before = await readWorkspaceDocument(page);
  const frontPoint = { xUm: 20_000, yUm: 27_000 };
  const backPoint = { xUm: 61_000, yUm: 34_000 };

  await page.getByTestId("production-context-front-silkscreen").click();
  await dropExternalPngAtBoardPoint(
    page.getByTestId("workspace-canvas-front"),
    before.board,
    frontPoint,
    filename,
  );

  const importer = page.getByRole("dialog", { name: "图片导入处理器" });
  await expect(importer).toBeVisible();
  await expect(importer.getByText("原图", { exact: true })).toBeVisible();
  await expect(importer.getByText("处理结果", { exact: true })).toBeVisible();
  const confirm = importer.getByRole("button", { name: "确认处理并插入" });
  await expect(confirm).toBeEnabled();
  await importer.getByRole("checkbox", { name: "反相" }).click();
  await expect(confirm).toBeDisabled();
  await expect(confirm).toBeEnabled();
  await confirm.click();
  await expect(page.getByRole("status")).toContainText(
    "图片已处理并放置到拖放位置",
  );

  const afterFrontPlacement = await readWorkspaceDocument(page);
  const frontImage = findNewImageLayer(
    before.frontLayers,
    afterFrontPlacement.frontLayers,
  );
  const asset = afterFrontPlacement.assets.find(
    (candidate) => candidate.id === frontImage.kind.assetId,
  );
  expect(asset).toBeTruthy();
  expectPointNear(layerCenter(frontImage.transform), frontPoint);

  await page.getByTestId("production-context-back-silkscreen").click();
  const mediaCard = page.getByRole("region", { name: "项目媒体" }).getByRole(
    "button",
    {
      name: `${asset!.originalFilename}，点击预览，可拖到背面丝印层`,
    },
  );
  await mediaCard.scrollIntoViewIfNeeded();
  await dragProjectAssetToBoardPoint(
    mediaCard,
    page.getByTestId("workspace-canvas-back"),
    afterFrontPlacement.board,
    backPoint,
  );
  await expect(importer).toBeVisible();
  await expect(confirm).toBeEnabled();
  await confirm.click();
  await expect(page.getByRole("status")).toContainText(
    "图片已处理并放置到拖放位置",
  );

  const afterBackPlacement = await readWorkspaceDocument(page);
  const backImage = findNewImageLayer(
    afterFrontPlacement.backLayers,
    afterBackPlacement.backLayers,
    asset!.id,
  );
  expectPointNear(layerCenter(backImage.transform), backPoint);
  expect(backImage.id).not.toBe(frontImage.id);
  expect(
    afterBackPlacement.assets.filter((candidate) => candidate.id === asset!.id),
  ).toHaveLength(1);
  expect(
    afterBackPlacement.imageTreatments.filter(
      (treatment) => treatment.assetId === asset!.id,
    ),
  ).toHaveLength(
    before.imageTreatments.filter(
      (treatment) => treatment.assetId === asset!.id,
    ).length + 2,
  );
  await expect(page.getByTestId("workspace-canvas-front")).toBeVisible();
  await expect(page.getByTestId("workspace-canvas-back")).toBeVisible();

  await page.getByTestId("production-context-front-silkscreen").click();
  await openObjectMenuAtLayer(
    page,
    page.getByTestId("workspace-canvas-front"),
    afterBackPlacement.board,
    frontImage,
  );
  const objectMenu = page.getByRole("menu", { name: "对象菜单" });
  await expect(objectMenu).toBeVisible();
  await objectMenu.getByRole("menuitem", { name: "关联到铜层" }).click();
  await expect(page.getByRole("status")).toContainText("对象已关联到铜层");

  const composed = await readWorkspaceDocument(page);
  const flowMappings = composed.mappings.filter(
    (mapping) =>
      mapping.sourceLayerId === frontImage.id ||
      mapping.sourceLayerId === backImage.id,
  );
  expect(flowMappings).toHaveLength(3);
  expect(
    flowMappings.map(({ sourceLayerId, target }) => ({
      sourceLayerId,
      side: target.side,
      layer: target.layer,
    })),
  ).toEqual(
    expect.arrayContaining([
      {
        sourceLayerId: frontImage.id,
        side: "front",
        layer: "silkscreen",
      },
      {
        sourceLayerId: frontImage.id,
        side: "front",
        layer: "copper",
      },
      {
        sourceLayerId: backImage.id,
        side: "back",
        layer: "silkscreen",
      },
    ]),
  );

  await page
    .getByRole("group", { name: "工作模式" })
    .getByRole("button", { name: "预览" })
    .click();
  const preview = page.getByTestId("board-3d-preview");
  await expect(preview).toBeVisible({ timeout: 15_000 });
  await expect(preview.locator("canvas")).toBeVisible();
  await expect(preview).toHaveAttribute(
    "data-fabrication-output-sha",
    /^[a-f0-9]{64}$/,
  );

  const outputDirectory = testInfo.outputPath("easyeda-formal");
  fs.mkdirSync(outputDirectory, { recursive: true });
  const report = await exportEasyeda(page, outputDirectory);
  expect(report.productionSource).toBe("formalProduction");
  expect(report.manufacturing.validated).toBe(true);
  expect(["directOrderSupported", "requiresManualAdjustment"]).toContain(
    report.orderSupport.status,
  );
  expect(report.orderSupport.directOrderSupported).toBe(
    report.orderSupport.status === "directOrderSupported",
  );
  expect(report.publicValidation).toMatchObject({ isValid: true, errors: [] });
  expect(report.nativeValidation).toMatchObject({ isValid: true, errors: [] });
  expect(report.fabricationInputSha256).toMatch(/^[a-f0-9]{64}$/);
  expect(report.fabricationOutputSha256).toMatch(/^[a-f0-9]{64}$/);

  const flowTraces = report.imageGraphics.filter(
    (trace) =>
      trace.sourceInstanceId === frontImage.id ||
      trace.sourceInstanceId === backImage.id,
  );
  expect(flowTraces).toHaveLength(3);
  expect(new Set(flowTraces.map((trace) => trace.assetSha256))).toEqual(
    new Set([asset!.sha256]),
  );
  expect(new Set(flowTraces.map((trace) => trace.sourceInstanceId))).toEqual(
    new Set([frontImage.id, backImage.id]),
  );
  expect(
    flowTraces.map(({ sourceInstanceId, target }) => ({
      sourceInstanceId,
      side: target.side,
      layer: target.layer,
    })),
  ).toEqual(
    expect.arrayContaining([
      {
        sourceInstanceId: frontImage.id,
        side: "front",
        layer: "silkscreen",
      },
      {
        sourceInstanceId: frontImage.id,
        side: "front",
        layer: "copper",
      },
      {
        sourceInstanceId: backImage.id,
        side: "back",
        layer: "silkscreen",
      },
    ]),
  );
  for (const trace of flowTraces) {
    expect(trace.treatmentId).toBeTruthy();
    expect(trace.algorithmVersion).toBe("atelier-image-treatment-v2");
    expect(trace.recipeFingerprint).toMatch(/^[a-f0-9]{64}$/);
    expect(trace.maskSha256).toMatch(/^[a-f0-9]{64}$/);
  }
  for (const artifact of [
    report.manifestPath,
    report.publicArchivePath,
    report.nativeProjectPath,
  ]) {
    expect(path.resolve(artifact).startsWith(path.resolve(outputDirectory))).toBe(
      true,
    );
    expect(fs.existsSync(artifact)).toBe(true);
  }
});

interface WorkspaceDocument {
  board: { widthUm: number; heightUm: number };
  frontLayers: ContentLayer[];
  backLayers: ContentLayer[];
  assets: Array<{
    id: string;
    originalFilename: string;
    sha256: string;
  }>;
  imageTreatments: Array<{ id: string; assetId: string }>;
  mappings: Array<{
    sourceLayerId: string;
    target: {
      side: "front" | "back";
      layer: "copper" | "solderMaskOpen" | "silkscreen";
    };
  }>;
}

interface ContentLayer {
  id: string;
  name: string;
  kind: { type: string; assetId?: string };
  transform: {
    xUm: number;
    yUm: number;
    widthUm: number;
    heightUm: number;
  };
}

interface EasyedaReport {
  manifestPath: string;
  publicArchivePath: string;
  nativeProjectPath: string;
  fabricationInputSha256: string;
  fabricationOutputSha256: string;
  productionSource: "formalProduction";
  manufacturing: { validated: boolean };
  orderSupport: {
    status: "directOrderSupported" | "requiresManualAdjustment";
    directOrderSupported: boolean;
  };
  publicValidation: { isValid: boolean; errors: string[] };
  nativeValidation: { isValid: boolean; errors: string[] };
  imageGraphics: Array<{
    sourceInstanceId: string;
    target: {
      side: "front" | "back";
      layer: "copper" | "solderMaskOpen" | "silkscreen";
    };
    treatmentId: string | null;
    algorithmVersion: string | null;
    recipeFingerprint: string | null;
    assetSha256: string;
    maskSha256: string;
  }>;
}

function findNewImageLayer(
  before: ContentLayer[],
  after: ContentLayer[],
  assetId?: string,
) {
  const previousIds = new Set(before.map(({ id }) => id));
  const layer = after.find(
    (candidate) =>
      !previousIds.has(candidate.id) &&
      candidate.kind.type === "image" &&
      (assetId === undefined || candidate.kind.assetId === assetId),
  );
  if (!layer) throw new Error(`未找到素材 ${assetId ?? "未知"} 的新图片实例`);
  return layer;
}

async function readWorkspaceDocument(page: Page) {
  return invokeBridge<WorkspaceDocument>(
    page,
    "get_workspace_document",
    {},
  );
}

async function exportEasyeda(page: Page, outputDirectory: string) {
  return invokeBridge<EasyedaReport>(page, "export_easyeda", {
    outputDirectory,
  });
}

async function invokeBridge<T>(
  page: Page,
  command: string,
  args: Record<string, unknown>,
) {
  return page.evaluate(
    async ({ bridgeCommand, bridgeArgs }) => {
      const response = await fetch("/__atelier_bridge", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          contractVersion: "pcb-atelier-workspace-v1",
          command: bridgeCommand,
          args: bridgeArgs,
        }),
      });
      const result = (await response.json()) as {
        payload: T;
        error: string | null;
      };
      if (!response.ok || result.error) {
        throw new Error(result.error ?? `bridge HTTP ${response.status}`);
      }
      return result.payload;
    },
    { bridgeCommand: command, bridgeArgs: args },
  );
}

async function dropExternalPngAtBoardPoint(
  target: Locator,
  board: { widthUm: number; heightUm: number },
  point: { xUm: number; yUm: number },
  filename: string,
) {
  const base64 = fs
    .readFileSync(
      path.resolve(process.cwd(), "../../assets/branding/pcb-atelier-logo.png"),
    )
    .toString("base64");
  const position = await boardPointToRelativeClient(target, board, point);
  await target.evaluate(
    (element, request) => {
      const binary = atob(request.base64);
      const bytes = Uint8Array.from(binary, (character) =>
        character.charCodeAt(0),
      );
      const dataTransfer = new DataTransfer();
      dataTransfer.items.add(
        new File([bytes], request.filename, { type: "image/png" }),
      );
      const bounds = element.getBoundingClientRect();
      for (const type of ["dragenter", "dragover", "drop"]) {
        element.dispatchEvent(
          new DragEvent(type, {
            bubbles: true,
            cancelable: true,
            clientX: bounds.left + request.position.x,
            clientY: bounds.top + request.position.y,
            dataTransfer,
          }),
        );
      }
    },
    { base64, filename, position },
  );
}

async function dragProjectAssetToBoardPoint(
  source: Locator,
  target: Locator,
  board: { widthUm: number; heightUm: number },
  point: { xUm: number; yUm: number },
) {
  const position = await boardPointToRelativeClient(target, board, point);
  await source.dragTo(target, { targetPosition: position });
}

async function openObjectMenuAtLayer(
  page: Page,
  canvas: Locator,
  board: { widthUm: number; heightUm: number },
  layer: ContentLayer,
) {
  const position = await boardPointToRelativeClient(
    canvas,
    board,
    layerCenter(layer.transform),
  );
  await canvas.click({ button: "right", position });
  await expect(page.getByRole("menu", { name: "对象菜单" })).toBeVisible();
}

async function boardPointToRelativeClient(
  canvas: Locator,
  board: { widthUm: number; heightUm: number },
  point: { xUm: number; yUm: number },
) {
  return canvas.evaluate((element, request) => {
    const bounds = element.getBoundingClientRect();
    const zoom = Number((element as HTMLElement).dataset.viewportZoom);
    const panX = Number((element as HTMLElement).dataset.viewportPanX);
    const panY = Number((element as HTMLElement).dataset.viewportPanY);
    const scale = 5.5 * zoom;
    return {
      x:
        bounds.width / 2 -
        (request.board.widthUm / 1_000) * scale / 2 +
        panX +
        (request.point.xUm / 1_000) * scale,
      y:
        bounds.height / 2 -
        (request.board.heightUm / 1_000) * scale / 2 +
        panY +
        (request.point.yUm / 1_000) * scale,
    };
  }, { board, point });
}

function layerCenter(transform: ContentLayer["transform"]) {
  return {
    xUm: transform.xUm + Math.floor(transform.widthUm / 2),
    yUm: transform.yUm + Math.floor(transform.heightUm / 2),
  };
}

function expectPointNear(
  actual: { xUm: number; yUm: number },
  expected: { xUm: number; yUm: number },
) {
  // A DOM drop is quantized to a device pixel before the Rust bridge receives it.
  expect(Math.abs(actual.xUm - expected.xUm)).toBeLessThanOrEqual(250);
  expect(Math.abs(actual.yUm - expected.yUm)).toBeLessThanOrEqual(250);
}
