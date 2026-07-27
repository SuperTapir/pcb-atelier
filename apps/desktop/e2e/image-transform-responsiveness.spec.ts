import path from "node:path";

import { expect, test, type Locator, type Page } from "./test";

interface LayerTransform {
  xUm: number;
  yUm: number;
  widthUm: number;
  heightUm: number;
  rotationMdeg: number;
}

interface WorkspaceDocumentView {
  board: { widthUm: number; heightUm: number };
  frontLayers: Array<{
    id: string;
    name: string;
    kind: { type: string };
    transform: LayerTransform;
  }>;
}

test("处理图片真实缩放在慢代理和失败代理期间保持纹理与连续帧", async ({
  page,
}) => {
  test.slow();
  let proxyRequests = 0;
  let holdNextProxy = false;
  let failNextProxy = false;
  let heldRequest:
    | {
        physicalWidthUm: number;
        physicalHeightUm: number;
        pixelPitchUm: number;
      }
    | undefined;
  let releaseHeldProxy!: () => void;
  const heldProxyReleased = new Promise<void>((resolve) => {
    releaseHeldProxy = resolve;
  });

  await page.route("**/__atelier_bridge", async (route) => {
    const payload = route.request().postDataJSON() as {
      command?: string;
      args?: {
        request?: {
          physicalWidthUm?: number;
          physicalHeightUm?: number;
          pixelPitchUm?: number;
        };
      };
    } | null;
    // The canvas texture uses the persisted treatment compiler. The inspector
    // independently uses request_image_preview for its own small preview.
    if (payload?.command !== "compile_image_treatment") {
      await route.continue();
      return;
    }
    proxyRequests += 1;
    if (failNextProxy) {
      failNextProxy = false;
      await route.fulfill({
        body: JSON.stringify({
          error: "injected proxy failure",
          payload: null,
        }),
        contentType: "application/json",
        status: 500,
      });
      return;
    }
    if (!holdNextProxy) {
      await route.continue();
      return;
    }

    holdNextProxy = false;
    heldRequest = {
      physicalWidthUm: payload.args?.request?.physicalWidthUm ?? 0,
      physicalHeightUm: payload.args?.request?.physicalHeightUm ?? 0,
      pixelPitchUm: payload.args?.request?.pixelPitchUm ?? 0,
    };
    const response = await route.fetch();
    await heldProxyReleased;
    await route.fulfill({ response });
  });

  await page.goto("/");
  await expect(
    page.locator("header").getByText("双面非对称黄金卡", { exact: true }),
  ).toBeVisible();
  await page.getByTestId("production-context-front-silkscreen").click();
  await page
    .getByTestId("image-file-input")
    .setInputFiles(
      path.resolve(
        process.cwd(),
        "benchmarks/interactive-image-v1/fixtures/tauri-editor-1003x1568.png",
      ),
    );
  const importer = page.getByRole("dialog", { name: "图片导入处理器" });
  const smoothing = importer.getByRole("slider", {
    name: "平滑半径 mm 快速调节",
  });
  await smoothing.fill("1");
  await smoothing.dispatchEvent("pointerup");
  const confirm = importer.getByRole("button", { name: "确认处理并插入" });
  await expect(confirm).toBeEnabled({ timeout: 15_000 });
  await confirm.click();
  await expect(page.getByRole("status")).toContainText("图片已处理并插入");

  const documentBefore = await readWorkspaceDocument(page);
  const imageLayer = documentBefore.frontLayers.find(
    (layer) =>
      layer.name === "tauri-editor-1003x1568.png" &&
      layer.kind.type === "image",
  );
  expect(imageLayer).toBeTruthy();
  const canvas = page.getByTestId("workspace-canvas-front");
  await expect(canvas).toBeVisible();
  await expect
    .poll(() =>
      countOpaqueImagePixels(
        page,
        canvas,
        documentBefore,
        imageLayer!.transform,
      ),
    )
    .toBeGreaterThan(100);

  await page.waitForTimeout(300);
  const requestsBeforeGesture = proxyRequests;
  const initialBounds = await imageScreenBounds(
    canvas,
    documentBefore,
    imageLayer!.transform,
  );
  holdNextProxy = true;
  const gestureFrames = sampleFrameIntervals(page, 45);
  await page.mouse.move(initialBounds.right, initialBounds.bottom);
  await page.mouse.down();
  for (let step = 1; step <= 24; step += 1) {
    await page.mouse.move(
      initialBounds.right - step * 1.5,
      initialBounds.bottom - step * 2,
    );
    await page.waitForTimeout(8);
  }
  expect(proxyRequests).toBe(requestsBeforeGesture);
  await page.mouse.up();
  await expect
    .poll(() => heldRequest, {
      message: "缩放提交后应启动新的画布代理编译",
      timeout: 15_000,
    })
    .toBeTruthy();

  const documentAfter = await readWorkspaceDocument(page);
  const resizedLayer = documentAfter.frontLayers.find(
    (layer) => layer.id === imageLayer!.id,
  );
  expect(resizedLayer).toBeTruthy();
  expect(resizedLayer!.transform.widthUm).toBeLessThan(
    imageLayer!.transform.widthUm,
  );
  expect(heldRequest?.physicalWidthUm).toBe(resizedLayer!.transform.widthUm);
  expect(heldRequest?.physicalHeightUm).toBe(resizedLayer!.transform.heightUm);

  const pendingVisual = await sampleOpaqueImageFrames(
    page,
    canvas,
    documentAfter,
    resizedLayer!.transform,
    30,
  );
  expect(pendingVisual.first).toBeGreaterThan(100);
  expect(pendingVisual.minimum).toBeGreaterThanOrEqual(
    pendingVisual.first * 0.8,
  );
  const gestureTiming = await gestureFrames;
  expect(percentile(gestureTiming, 0.95)).toBeLessThanOrEqual(34);
  expect(gestureTiming.filter((duration) => duration > 50)).toHaveLength(0);

  releaseHeldProxy();
  await expect.poll(() => proxyRequests).toBeGreaterThan(requestsBeforeGesture);
  await page.waitForTimeout(100);

  const pixelsBeforeFailure = await countOpaqueImagePixels(
    page,
    canvas,
    documentAfter,
    resizedLayer!.transform,
  );
  failNextProxy = true;
  const widthInput = page.getByRole("textbox", { name: "宽 (mm)" });
  await widthInput.fill(
    (resizedLayer!.transform.widthUm / 1_000 - 1).toFixed(3),
  );
  await widthInput.press("Enter");
  await expect.poll(() => failNextProxy).toBe(false);
  const documentAfterFailure = await readWorkspaceDocument(page);
  const failedResizeLayer = documentAfterFailure.frontLayers.find(
    (layer) => layer.id === imageLayer!.id,
  );
  expect(failedResizeLayer).toBeTruthy();
  const pixelsAfterFailure = await countOpaqueImagePixels(
    page,
    canvas,
    documentAfterFailure,
    failedResizeLayer!.transform,
  );
  expect(pixelsAfterFailure).toBeGreaterThanOrEqual(pixelsBeforeFailure * 0.7);
});

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
      payload: WorkspaceDocumentView;
      error: string | null;
    };
    if (result.error) throw new Error(result.error);
    return result.payload;
  });
}

async function imageScreenBounds(
  canvas: Locator,
  document: WorkspaceDocumentView,
  transform: LayerTransform,
) {
  const box = await canvas.boundingBox();
  if (!box) throw new Error("正面画布不可见");
  const zoom = Number(await canvas.getAttribute("data-viewport-zoom"));
  const scale = 5.5 * zoom;
  const originX =
    box.width / 2 - ((document.board.widthUm / 1_000) * scale) / 2;
  const originY =
    box.height / 2 - ((document.board.heightUm / 1_000) * scale) / 2;
  const left = box.x + originX + (transform.xUm / 1_000) * scale;
  const top = box.y + originY + (transform.yUm / 1_000) * scale;
  return {
    left,
    top,
    right: left + (transform.widthUm / 1_000) * scale,
    bottom: top + (transform.heightUm / 1_000) * scale,
  };
}

async function countOpaqueImagePixels(
  page: Page,
  canvas: Locator,
  document: WorkspaceDocumentView,
  transform: LayerTransform,
) {
  const bounds = await imageScreenBounds(canvas, document, transform);
  return canvas.evaluate((container, screenBounds) => {
    const contentCanvas = [...container.querySelectorAll("canvas")].at(-1);
    if (!contentCanvas) return 0;
    const context = contentCanvas.getContext("2d", {
      willReadFrequently: true,
    });
    if (!context) return 0;
    const canvasBounds = contentCanvas.getBoundingClientRect();
    const scaleX = contentCanvas.width / canvasBounds.width;
    const scaleY = contentCanvas.height / canvasBounds.height;
    // Ignore the selected-node outline and Transformer anchors. A missing
    // texture still paints those controls, so counting the full rectangle
    // would let a placeholder produce a false positive.
    const insetCssPx = 4;
    const x = Math.max(
      0,
      Math.floor((screenBounds.left + insetCssPx - canvasBounds.left) * scaleX),
    );
    const y = Math.max(
      0,
      Math.floor((screenBounds.top + insetCssPx - canvasBounds.top) * scaleY),
    );
    const width = Math.max(
      1,
      Math.min(
        contentCanvas.width - x,
        Math.ceil(
          (screenBounds.right - screenBounds.left - insetCssPx * 2) * scaleX,
        ),
      ),
    );
    const height = Math.max(
      1,
      Math.min(
        contentCanvas.height - y,
        Math.ceil(
          (screenBounds.bottom - screenBounds.top - insetCssPx * 2) * scaleY,
        ),
      ),
    );
    const pixels = context.getImageData(x, y, width, height).data;
    let opaque = 0;
    for (let index = 3; index < pixels.length; index += 4) {
      if (pixels[index]! >= 220) opaque += 1;
    }
    return opaque;
  }, bounds);
}

async function sampleOpaqueImageFrames(
  page: Page,
  canvas: Locator,
  document: WorkspaceDocumentView,
  transform: LayerTransform,
  frameCount: number,
) {
  const samples: number[] = [];
  for (let frame = 0; frame < frameCount; frame += 1) {
    await page.evaluate(() => new Promise(requestAnimationFrame));
    samples.push(
      await countOpaqueImagePixels(page, canvas, document, transform),
    );
  }
  return {
    first: samples[0] ?? 0,
    minimum: Math.min(...samples),
  };
}

async function sampleFrameIntervals(page: Page, frameCount: number) {
  return page.evaluate(async (count) => {
    const timestamps: number[] = [];
    for (let frame = 0; frame <= count; frame += 1) {
      timestamps.push(
        await new Promise<number>((resolve) => requestAnimationFrame(resolve)),
      );
    }
    return timestamps
      .slice(1)
      .map((value, index) => value - timestamps[index]!);
  }, frameCount);
}

function percentile(values: number[], ratio: number) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[
    Math.min(sorted.length - 1, Math.ceil(sorted.length * ratio) - 1)
  ]!;
}
