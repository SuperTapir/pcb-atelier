import { expect, test, type Locator, type Page } from "./test";
import fs from "node:fs";
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

test("真实右键手势区分原地菜单和平移，并保持正背独立视口", async ({
  page,
}) => {
  const front = page.getByTestId("workspace-canvas-front");
  const back = page.getByTestId("workspace-canvas-back");
  const frontBox = await requiredBox(front);
  const viewportBeforeClick = await readViewport(front);
  const clickStart = { x: frontBox.x + 22, y: frontBox.y + 22 };
  const clickEnd = { x: clickStart.x + 3, y: clickStart.y + 2 };

  await page.mouse.move(clickStart.x, clickStart.y);
  await page.mouse.down({ button: "right" });
  await page.mouse.move(clickEnd.x, clickEnd.y);
  await page.mouse.up({ button: "right" });

  const menu = page.getByRole("menu", { name: "画布菜单" });
  await expect(menu).toBeVisible();
  expect(await readViewport(front)).toEqual(viewportBeforeClick);
  const menuBox = await requiredBox(menu);
  expect(Math.abs(menuBox.x - clickEnd.x)).toBeLessThanOrEqual(1);
  expect(Math.abs(menuBox.y - clickEnd.y)).toBeLessThanOrEqual(1);

  await page.getByRole("heading", { name: "检查器" }).click();
  await expect(menu).toHaveCount(0);
  await page.reload();
  await expect(front).toBeVisible();

  const frontBefore = await readViewport(front);
  await rightDrag(page, front, 46, 28);
  await expect
    .poll(async () => (await readViewport(front)).panX)
    .toBeCloseTo(frontBefore.panX + 46, 0);
  const frontAfter = await readViewport(front);
  expect(frontAfter.panX).toBeCloseTo(frontBefore.panX + 46, 0);
  expect(frontAfter.panY).toBeCloseTo(frontBefore.panY + 28, 0);
  await expect(menu).toHaveCount(0);

  const backBefore = await readViewport(back);
  expect(backBefore.panX).not.toBe(frontAfter.panX);
  await back.click({ position: { x: 18, y: 18 } });
  await expect(back).toHaveAttribute("data-active", "true");
  expect(await readViewport(back)).toEqual(backBefore);
  expect(await readViewport(front)).toEqual(frontAfter);
});

test("真实左键框选命中画板对象且不改变对象几何", async ({ page }) => {
  const before = await readWorkspaceDocument(page);
  const canvas = page.getByTestId("workspace-canvas-front");
  const canvasBox = await requiredBox(canvas);
  const viewport = await readViewport(canvas);
  const start = boardPointToClient(
    canvasBox,
    before.board,
    viewport,
    { xUm: 5_000, yUm: 5_000 },
  );
  const end = boardPointToClient(
    canvasBox,
    before.board,
    viewport,
    { xUm: 38_000, yUm: 16_000 },
  );

  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  await page.mouse.move(end.x, end.y, { steps: 10 });
  await page.mouse.up();

  const tree = page.getByRole("tree", { name: "板体与生产层" });
  await expect(
    tree.getByRole("treeitem").filter({ hasText: "正面标题" }),
  ).toHaveAttribute("aria-selected", "true");
  await expect(
    tree.getByRole("treeitem").filter({ hasText: "正面说明" }),
  ).toHaveAttribute("aria-selected", "true");
  const after = await readWorkspaceDocument(page);
  expect(after.frontLayers).toEqual(before.frontLayers);
});

test("连续滚轮 delta 保持指针锚点，且快/标准/慢缩放幅度有序", async ({
  page,
}) => {
  const deltas: Record<"high" | "medium" | "low", number> = {
    high: 0,
    medium: 0,
    low: 0,
  };

  for (const damping of ["high", "medium", "low"] as const) {
    await setWheelDamping(page, damping);
    const canvas = page.getByTestId("workspace-canvas-front");
    const bounds = await requiredBox(canvas);
    const pointer = {
      x: bounds.x + bounds.width * 0.72,
      y: bounds.y + bounds.height * 0.38,
    };
    const before = await readViewport(canvas);
    const anchorBefore = clientToBoardPoint(
      bounds,
      (await readWorkspaceDocument(page)).board,
      before,
      pointer,
    );

    await page.mouse.move(pointer.x, pointer.y);
    await page.mouse.wheel(0, -20);
    const first = await waitForZoomChange(canvas, before.zoom);
    await page.mouse.wheel(0, -80);
    const second = await waitForZoomChange(canvas, first.zoom);
    expect(first.zoom).toBeGreaterThan(before.zoom);
    expect(second.zoom).toBeGreaterThan(first.zoom);

    const anchorAfter = clientToBoardPoint(
      bounds,
      (await readWorkspaceDocument(page)).board,
      second,
      pointer,
    );
    expect(Math.abs(anchorAfter.xUm - anchorBefore.xUm)).toBeLessThan(25);
    expect(Math.abs(anchorAfter.yUm - anchorBefore.yUm)).toBeLessThan(25);
    deltas[damping] = second.zoom - before.zoom;
  }

  expect(deltas.low).toBeGreaterThan(deltas.medium);
  expect(deltas.medium).toBeGreaterThan(deltas.high);
});

test("外部图片只导入媒体库，媒体素材真实拖到画板并原子拒绝无效落点", async ({
  page,
}) => {
  test.setTimeout(60_000);
  const beforeImport = await readWorkspaceDocument(page);
  const media = page.getByRole("region", { name: "项目媒体" });
  await dropExternalPng(media, "media-dnd-source.png");
  await expect(page.getByRole("status")).toContainText(
    "1 个素材已导入媒体库",
  );

  const afterImport = await readWorkspaceDocument(page);
  expect(afterImport.assets).toHaveLength(beforeImport.assets.length + 1);
  expect(afterImport.frontLayers).toEqual(beforeImport.frontLayers);
  expect(afterImport.backLayers).toEqual(beforeImport.backLayers);
  expect(afterImport.imageTreatments).toEqual(beforeImport.imageTreatments);
  expect(afterImport.mappings).toEqual(beforeImport.mappings);
  const asset = afterImport.assets.find(
    (candidate) => candidate.originalFilename === "media-dnd-source.png",
  );
  expect(asset).toBeTruthy();

  const mediaCard = media.getByRole("button", {
    name: "media-dnd-source.png，点击预览，可拖到正面丝印层",
  });
  const canvas = page.getByTestId("workspace-canvas-front");
  const canvasBox = await requiredBox(canvas);
  await mediaCard.dragTo(canvas, {
    targetPosition: { x: canvasBox.width / 2, y: canvasBox.height / 2 },
  });
  const importer = page.getByRole("dialog", { name: "图片导入处理器" });
  await expect(importer).toBeVisible();
  await importer.getByRole("button", { name: "确认处理并插入" }).click();
  await expect(
    page.getByText("图片已处理并放置到拖放位置", { exact: true }),
  ).toBeVisible();

  const afterPlacement = await readWorkspaceDocument(page);
  expect(afterPlacement.assets).toHaveLength(afterImport.assets.length);
  expect(afterPlacement.frontLayers).toHaveLength(
    afterImport.frontLayers.length + 1,
  );
  expect(afterPlacement.mappings).toHaveLength(afterImport.mappings.length + 1);
  expect(
    afterPlacement.frontLayers.some(
      (layer) =>
        layer.kind.type === "image" && layer.kind.assetId === asset!.id,
    ),
  ).toBe(true);

  await mediaCard.evaluate((source) => {
    const target = document.querySelector<HTMLElement>(
      '[data-testid="workspace-canvas-front"]',
    );
    if (!target) throw new Error("front canvas is missing");
    const dataTransfer = new DataTransfer();
    source.dispatchEvent(
      new DragEvent("dragstart", {
        bubbles: true,
        cancelable: true,
        dataTransfer,
      }),
    );
    const bounds = target.getBoundingClientRect();
    for (const type of ["dragover", "drop"]) {
      target.dispatchEvent(
        new DragEvent(type, {
          bubbles: true,
          cancelable: true,
          clientX: bounds.left + 4,
          clientY: bounds.top + 4,
          dataTransfer,
        }),
      );
    }
    source.dispatchEvent(
      new DragEvent("dragend", { bubbles: true, dataTransfer }),
    );
  });
  await expect(page.getByRole("status")).toContainText("图片必须拖到板框内");
  expect(await readWorkspaceDocument(page)).toEqual(afterPlacement);
});

test("媒体素材可以真实拖到指定生产层并进入对应图片处理流程", async ({
  page,
}) => {
  const media = page.getByRole("region", { name: "项目媒体" });
  await dropExternalPng(media, "media-to-tree.png");
  await expect(
    page.getByText("1 个素材已导入媒体库", { exact: true }),
  ).toBeVisible();

  const source = media.getByRole("button", {
    name: "media-to-tree.png，点击预览，可拖到正面丝印层",
  });
  const target = page.getByTestId("production-layer-back-copper");
  await source.dragTo(target);

  const importer = page.getByRole("dialog", { name: "图片导入处理器" });
  await expect(importer).toBeVisible();
  await importer.getByRole("button", { name: "取消" }).click();
  await expect(
    page.getByText("已取消图片导入", { exact: true }),
  ).toBeVisible();
});

async function setWheelDamping(
  page: Page,
  wheelZoomDamping: "high" | "medium" | "low",
) {
  await page.evaluate((value) => {
    localStorage.setItem(
      "pcb-atelier.app-settings.v2",
      JSON.stringify({
        canvasView: "horizontal",
        launchWindowMode: "maximized",
        wheelZoomDamping: value,
      }),
    );
  }, wheelZoomDamping);
  await page.reload();
  await expect(page.getByTestId("workspace-canvas-front")).toBeVisible();
}

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
}

async function requiredBox(locator: Locator) {
  const box = await locator.boundingBox();
  if (!box) throw new Error("目标不可见");
  return box;
}

async function dropExternalPng(target: Locator, filename: string) {
  const base64 = fs
    .readFileSync(
      path.resolve(process.cwd(), "../../assets/branding/pcb-atelier-logo.png"),
    )
    .toString("base64");
  await target.evaluate((element, request) => {
    const binary = atob(request.base64);
    const bytes = Uint8Array.from(binary, (character) =>
      character.charCodeAt(0),
    );
    const dataTransfer = new DataTransfer();
    dataTransfer.items.add(
      new File([bytes], request.filename, { type: "image/png" }),
    );
    for (const type of ["dragenter", "dragover", "drop"]) {
      element.dispatchEvent(
        new DragEvent(type, {
          bubbles: true,
          cancelable: true,
          dataTransfer,
        }),
      );
    }
  }, { base64, filename });
}

async function readViewport(canvas: Locator) {
  return canvas.evaluate((element) => ({
    panX: Number((element as HTMLElement).dataset.viewportPanX),
    panY: Number((element as HTMLElement).dataset.viewportPanY),
    zoom: Number((element as HTMLElement).dataset.viewportZoom),
  }));
}

async function waitForZoomChange(canvas: Locator, previous: number) {
  await expect
    .poll(async () => (await readViewport(canvas)).zoom)
    .not.toBe(previous);
  return readViewport(canvas);
}

function boardPointToClient(
  bounds: { x: number; y: number; width: number; height: number },
  board: { widthUm: number; heightUm: number },
  viewport: { panX: number; panY: number; zoom: number },
  point: { xUm: number; yUm: number },
) {
  const scale = 5.5 * viewport.zoom;
  return {
    x:
      bounds.x +
      bounds.width / 2 -
      (board.widthUm / 1_000) * scale / 2 +
      viewport.panX +
      (point.xUm / 1_000) * scale,
    y:
      bounds.y +
      bounds.height / 2 -
      (board.heightUm / 1_000) * scale / 2 +
      viewport.panY +
      (point.yUm / 1_000) * scale,
  };
}

function clientToBoardPoint(
  bounds: { x: number; y: number; width: number; height: number },
  board: { widthUm: number; heightUm: number },
  viewport: { panX: number; panY: number; zoom: number },
  point: { x: number; y: number },
) {
  const scale = 5.5 * viewport.zoom;
  const originX =
    bounds.x +
    bounds.width / 2 -
    (board.widthUm / 1_000) * scale / 2 +
    viewport.panX;
  const originY =
    bounds.y +
    bounds.height / 2 -
    (board.heightUm / 1_000) * scale / 2 +
    viewport.panY;
  return {
    xUm: ((point.x - originX) / scale) * 1_000,
    yUm: ((point.y - originY) / scale) * 1_000,
  };
}

async function readWorkspaceDocument(page: Page) {
  return page.evaluate(async () => {
    const response = await fetch("/__atelier_bridge", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        contractVersion: "pcb-atelier-workspace-v1",
        command: "get_workspace_document",
        args: {},
      }),
    });
    const result = (await response.json()) as {
      payload: {
        board: { widthUm: number; heightUm: number };
        frontLayers: Array<{
          id: string;
          kind:
            | { type: "image"; assetId: string }
            | { type: string };
        }>;
        backLayers: unknown[];
        assets: Array<{ id: string; originalFilename: string }>;
        imageTreatments: unknown[];
        mappings: unknown[];
      };
    };
    return result.payload;
  });
}
