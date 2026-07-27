import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { performance } from "node:perf_hooks";
import { chromium } from "@playwright/test";

const desktopRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const workspaceRoot = resolve(desktopRoot, "../..");
const address = "127.0.0.1:1434";
const baseUrl = `http://${address}`;
const fixturePath = resolve(
  desktopRoot,
  "benchmarks/interactive-image-v1/fixtures/tauri-editor-1003x1568.png",
);
const bridge = spawn(
  "cargo",
  ["run", "--release", "-p", "atelier-desktop", "--bin", "workspace-bridge"],
  {
    cwd: workspaceRoot,
    env: { ...process.env, PCB_ATELIER_BRIDGE_ADDR: address },
    stdio: ["ignore", "ignore", "inherit"],
  },
);
let browser;

try {
  await waitForHealth();
  browser = await chromium.launch({ channel: "chrome", headless: true });
  const page = await browser.newPage();
  const bytes = [...(await readFile(fixturePath))];
  const beginRequest = request("begin_image_preview_source", {
    bytes,
    mediaType: "image/png",
  });
  const source = await invoke(beginRequest);
  const baseRecipe = {
    algorithmVersion: "atelier-image-treatment-v2",
    alphaMode: "compositeOnWhite",
    threshold: { mode: "manual", value: 96 },
    invert: false,
    smoothingRadiusUm: 0,
    despeckleRadiusUm: 0,
    removeIslandsBelowUm2: 0,
    minimumLineWidthUm: 0,
    thinFeaturePolicy: "preserve",
    minimumGapUm: 0,
    crop: null,
  };
  const makePreview = (generation, threshold, invert = false) =>
    request("request_image_preview", {
      sourceHandle: source.sourceHandle,
      previewStreamId: "interactive-image-v1",
      generation,
      workspaceRevision: source.workspaceRevision,
      recipe: {
        ...baseRecipe,
        threshold: { mode: "manual", value: threshold },
        invert,
      },
      physicalWidthUm: 51_170,
      physicalHeightUm: 80_000,
      pixelPitchUm: 250,
    });

  await invoke(makePreview(1, 96));
  const latenciesMs = [];
  const bridgeLatenciesMs = [];
  for (let sample = 0; sample < 30; sample += 1) {
    const started = performance.now();
    const report = await invoke(
      makePreview(sample + 2, 97 + sample, sample % 2 === 1),
    );
    bridgeLatenciesMs.push(performance.now() - started);
    await page.evaluate(async (dataUrl) => {
      const response = await fetch(dataUrl);
      const bitmap = await createImageBitmap(await response.blob());
      bitmap.close();
    }, report.previewPngDataUrl);
    latenciesMs.push(performance.now() - started);
  }

  const inputDispatchMs = [];
  const burst = Array.from({ length: 60 }, (_, index) =>
    (() => {
      const started = performance.now();
      const pending = invokeAllowCancellation(
        makePreview(index + 32, 140 + index),
      );
      inputDispatchMs.push(performance.now() - started);
      return pending;
    })(),
  );
  const burstResults = await Promise.all(burst);
  const finalBurst = burstResults.at(-1);
  if (finalBurst.error) {
    throw new Error(`final generation failed: ${finalBurst.error}`);
  }
  const resizeBurstStarted = performance.now();
  const resizeBurst = Array.from({ length: 20 }, (_, index) =>
    invokeAllowCancellation(
      request("request_image_preview", {
        sourceHandle: source.sourceHandle,
        previewStreamId: "interactive-image-resize-v1",
        generation: index + 1,
        workspaceRevision: source.workspaceRevision,
        recipe: {
          ...baseRecipe,
          smoothingRadiusUm: 1_000,
        },
        physicalWidthUm: 24_000 - index * 200,
        physicalHeightUm: 38_000 - index * 300,
        pixelPitchUm: 25,
      }),
    ),
  );
  const resizeBurstResults = await Promise.all(resizeBurst);
  const resizeBurstLatestMs = performance.now() - resizeBurstStarted;
  const finalResize = resizeBurstResults.at(-1);
  if (finalResize.error) {
    throw new Error(`final resize generation failed: ${finalResize.error}`);
  }
  const diagnostics = await invoke(
    request("get_image_preview_diagnostics", undefined),
  );
  await invoke(
    request("release_image_preview_source", {
      sourceHandle: source.sourceHandle,
    }),
  );

  const previewRequest = makePreview(92, 200);
  const result = {
    profile: "interactive-image-v1",
    fixture: fixturePath,
    sourceBytes: bytes.length,
    beginJsonBytes: Buffer.byteLength(JSON.stringify(beginRequest)),
    previewJsonBytes: Buffer.byteLength(JSON.stringify(previewRequest)),
    output: {
      widthPx: finalBurst.payload.widthPx,
      heightPx: finalBurst.payload.heightPx,
    },
    samples: latenciesMs.length,
    bridgeP95Ms: percentile(bridgeLatenciesMs, 0.95),
    drawableP50Ms: percentile(latenciesMs, 0.5),
    drawableP95Ms: percentile(latenciesMs, 0.95),
    inputDispatchP95Ms: percentile(inputDispatchMs, 0.95),
    finalGenerationAccepted: !finalBurst.error,
    cancelledBurstRequests: burstResults.filter((entry) => entry.error).length,
    resizeBurstLatestMs: Number(resizeBurstLatestMs.toFixed(2)),
    cancelledResizeRequests: resizeBurstResults.filter((entry) => entry.error)
      .length,
    resizeBurstSmoothingRadiusUm: 1_000,
    resizeBurstPixelPitchUm: 25,
    resizeOutput: {
      widthPx: finalResize.payload.widthPx,
      heightPx: finalResize.payload.heightPx,
    },
    diagnostics,
  };
  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
  if (result.drawableP95Ms > 120) process.exitCode = 1;
  if (result.inputDispatchP95Ms > 16.7) process.exitCode = 1;
  if (result.resizeBurstLatestMs > 250) process.exitCode = 1;
  if (result.cancelledResizeRequests < 18) process.exitCode = 1;
  if (diagnostics.prepareCount !== 1) process.exitCode = 1;
  if (diagnostics.sourceBytes !== bytes.length) process.exitCode = 1;
} finally {
  await browser?.close();
  bridge.kill("SIGTERM");
}

function request(command, requestArgs) {
  return {
    contractVersion: "pcb-atelier-workspace-v1",
    command,
    args: requestArgs === undefined ? {} : { request: requestArgs },
  };
}

async function invoke(body) {
  const response = await invokeAllowCancellation(body);
  if (response.error) throw new Error(response.error);
  return response.payload;
}

async function invokeAllowCancellation(body) {
  const response = await fetch(`${baseUrl}/workspace`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!response.ok) throw new Error(`bridge returned HTTP ${response.status}`);
  return response.json();
}

async function waitForHealth() {
  for (let attempt = 0; attempt < 240; attempt += 1) {
    try {
      const response = await fetch(`${baseUrl}/health`);
      if (response.ok) return;
    } catch {
      // Release compilation may still be running.
    }
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 250));
  }
  throw new Error("workspace bridge did not become ready");
}

function percentile(values, ratio) {
  const sorted = [...values].sort((left, right) => left - right);
  return Number(
    sorted[
      Math.min(sorted.length - 1, Math.ceil(sorted.length * ratio) - 1)
    ].toFixed(2),
  );
}
