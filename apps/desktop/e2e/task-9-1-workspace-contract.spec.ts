import { expect, test, type Locator, type Page } from "./test";
import path from "node:path";

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto("/");
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  await expect(
    page.locator("header").getByText("双面非对称黄金卡", { exact: true }),
  ).toBeVisible();
});

test("聚焦当前面切换正背时保留各自选择与独立视口", async ({ page }) => {
  const tree = page.getByRole("tree", { name: "板体与生产层" });
  const frontRow = tree.getByRole("treeitem").filter({ hasText: "正面说明" });
  const backRow = tree.getByRole("treeitem").filter({ hasText: "背面说明" });
  const front = page.getByTestId("workspace-canvas-front");
  const back = page.getByTestId("workspace-canvas-back");

  await frontRow
    .getByRole("button", { name: "正面说明", exact: true })
    .click();
  await rightDrag(page, front, 34, 18);
  const frontViewport = await readViewport(front);

  await backRow
    .getByRole("button", { name: "背面说明", exact: true })
    .click();
  await rightDrag(page, back, -28, 24);
  const backViewport = await readViewport(back);

  await page
    .getByRole("combobox", { name: "画板视图" })
    .selectOption("focus-active");
  await expect(front).toHaveCount(0);
  await expect(back).toBeVisible();
  await expect(backRow).toHaveAttribute("aria-selected", "true");
  expect(await readViewport(back)).not.toEqual(backViewport);
  const backFocusViewport = await readViewport(back);

  await frontRow
    .getByRole("button", { name: "正面说明", exact: true })
    .click();
  await expect(back).toHaveCount(0);
  await expect(front).toBeVisible();
  await expect(frontRow).toHaveAttribute("aria-selected", "true");
  expect(await readViewport(front)).toEqual(frontViewport);
  await rightDrag(page, front, 22, -16);
  const frontFocusViewport = await readViewport(front);

  await backRow
    .getByRole("button", { name: "背面说明", exact: true })
    .click();
  await expect(front).toHaveCount(0);
  await expect(back).toBeVisible();
  await expect(backRow).toHaveAttribute("aria-selected", "true");
  expect(await readViewport(back)).toEqual(backFocusViewport);

  await frontRow
    .getByRole("button", { name: "正面说明", exact: true })
    .click();
  await expect(front).toBeVisible();
  await expect(frontRow).toHaveAttribute("aria-selected", "true");
  expect(await readViewport(front)).toEqual(frontFocusViewport);
});

test("临时查看原图不改变配方变换或正式输出，多映射仍共享单一源对象", async ({
  page,
}) => {
  test.setTimeout(60_000);
  const beforeImport = await readWorkspaceDocument(page);
  await page.getByTestId("production-context-front-silkscreen").click();
  await page.getByTestId("image-file-input").setInputFiles(
    path.resolve(process.cwd(), "../../assets/branding/pcb-atelier-logo.png"),
  );
  const importer = page.getByRole("dialog", { name: "图片导入处理器" });
  await expect(importer).toBeVisible();
  const confirmImport = importer.getByRole("button", {
    name: "确认处理并插入",
  });
  await expect(confirmImport).toBeEnabled();
  await confirmImport.click();
  await expect(page.getByRole("status")).toContainText("图片已处理并插入");

  const inserted = await readWorkspaceDocument(page);
  const image = inserted.frontLayers.find(
    (layer) =>
      layer.kind.type === "image" &&
      !beforeImport.frontLayers.some((candidate) => candidate.id === layer.id),
  );
  if (!image) throw new Error("缺少刚插入的图片对象");
  const traceBeforeOriginal = await readProductionTrace(page);
  const treatmentEditor = page.getByRole("region", { name: "图片处理" });
  const original = treatmentEditor.getByRole("button", {
    name: "临时查看原图",
  });

  await original.press("Space");
  await expect(original).toHaveAttribute("aria-pressed", "true");
  expect(await readWorkspaceDocument(page)).toEqual(inserted);
  const traceWithOriginal = await readProductionTrace(page);
  expect(traceWithOriginal.fabricationInputSha256).toBe(
    traceBeforeOriginal.fabricationInputSha256,
  );
  expect(traceWithOriginal.fabricationOutputSha256).toBe(
    traceBeforeOriginal.fabricationOutputSha256,
  );

  await original.press("Space");
  await expect(original).toHaveAttribute("aria-pressed", "false");
  expect(await readWorkspaceDocument(page)).toEqual(inserted);

  await openObjectMenuAtLayer(
    page,
    page.getByTestId("workspace-canvas-front"),
    inserted.board,
    image,
  );
  await page
    .getByRole("menu", { name: "对象菜单" })
    .getByRole("menuitem", { name: "关联到铜层" })
    .click();
  await expect(page.getByRole("status")).toContainText("对象已关联到铜层");

  const multiMapped = await readWorkspaceDocument(page);
  expect(multiMapped.frontLayers).toEqual(inserted.frontLayers);
  expect(
    multiMapped.mappings.filter((mapping) => mapping.sourceLayerId === image.id),
  ).toHaveLength(2);
  await openObjectMenuAtLayer(
    page,
    page.getByTestId("workspace-canvas-front"),
    multiMapped.board,
    image,
  );
  await expect(page.getByRole("menu", { name: "对象菜单" })).toHaveCount(1);
  await expect(
    page.getByRole("heading", { name: "检查器" }).locator(".."),
  ).toContainText(image.name);
});

async function rightDrag(
  page: Page,
  canvas: Locator,
  deltaX: number,
  deltaY: number,
) {
  const bounds = await requiredBox(canvas);
  const start = { x: bounds.x + bounds.width / 2, y: bounds.y + 24 };
  await page.mouse.move(start.x, start.y);
  await page.mouse.down({ button: "right" });
  await page.mouse.move(start.x + deltaX, start.y + deltaY);
  await page.mouse.up({ button: "right" });
  await expect
    .poll(async () => (await readViewport(canvas)).panX)
    .not.toBe(0);
}

async function openObjectMenuAtLayer(
  page: Page,
  canvas: Locator,
  board: { widthUm: number; heightUm: number },
  layer: ContentLayer,
) {
  const bounds = await requiredBox(canvas);
  const viewport = await readViewport(canvas);
  const scale = 5.5 * viewport.zoom;
  const originX =
    bounds.width / 2 - (board.widthUm / 1_000) * scale / 2 + viewport.panX;
  const originY =
    bounds.height / 2 - (board.heightUm / 1_000) * scale / 2 + viewport.panY;
  const point = {
    x:
      bounds.x +
      originX +
      (layer.transform.xUm + layer.transform.widthUm / 2) / 1_000 * scale,
    y:
      bounds.y +
      originY +
      (layer.transform.yUm + layer.transform.heightUm / 2) / 1_000 * scale,
  };
  await page.mouse.move(point.x, point.y);
  await page.mouse.down({ button: "right" });
  await page.mouse.up({ button: "right" });
  await expect(page.getByRole("menu", { name: "对象菜单" })).toBeVisible();
}

async function requiredBox(locator: Locator) {
  const box = await locator.boundingBox();
  if (!box) throw new Error("目标不可见");
  return box;
}

async function readViewport(canvas: Locator) {
  return canvas.evaluate((element) => ({
    panX: Number((element as HTMLElement).dataset.viewportPanX),
    panY: Number((element as HTMLElement).dataset.viewportPanY),
    zoom: Number((element as HTMLElement).dataset.viewportZoom),
  }));
}

async function readWorkspaceDocument(page: Page) {
  return invokeBridge<WorkspaceDocument>(page, "get_workspace_document");
}

async function readProductionTrace(page: Page) {
  return invokeBridge<ProductionTrace>(page, "get_production_trace");
}

async function invokeBridge<T>(page: Page, command: string) {
  return page.evaluate(async (bridgeCommand) => {
    const response = await fetch("/__atelier_bridge", {
      body: JSON.stringify({
        args: {},
        command: bridgeCommand,
        contractVersion: "pcb-atelier-workspace-v1",
      }),
      headers: { "Content-Type": "application/json" },
      method: "POST",
    });
    const result = (await response.json()) as {
      error: string | null;
      payload: T;
    };
    if (result.error) throw new Error(result.error);
    return result.payload;
  }, command);
}

interface ContentLayer {
  id: string;
  name: string;
  kind: { type: string };
  transform: {
    xUm: number;
    yUm: number;
    widthUm: number;
    heightUm: number;
  };
}

interface WorkspaceDocument {
  board: { widthUm: number; heightUm: number };
  frontLayers: ContentLayer[];
  backLayers: ContentLayer[];
  assets: unknown[];
  imageTreatments: unknown[];
  mappings: Array<{ sourceLayerId: string }>;
}

interface ProductionTrace {
  fabricationInputSha256: string;
  fabricationOutputSha256: string;
}
