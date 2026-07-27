import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { performance } from "node:perf_hooks";

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

try {
  await waitForHealth();
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
  for (let sample = 0; sample < 30; sample += 1) {
    const started = performance.now();
    await invoke(makePreview(sample + 2, 97 + sample, sample % 2 === 1));
    latenciesMs.push(performance.now() - started);
  }

  const burst = Array.from({ length: 60 }, (_, index) =>
    invokeAllowCancellation(makePreview(index + 32, 140 + index)),
  );
  const burstResults = await Promise.all(burst);
  const finalBurst = burstResults.at(-1);
  if (finalBurst.error) {
    throw new Error(`final generation failed: ${finalBurst.error}`);
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
    p50Ms: percentile(latenciesMs, 0.5),
    p95Ms: percentile(latenciesMs, 0.95),
    finalGenerationAccepted: !finalBurst.error,
    cancelledBurstRequests: burstResults.filter((entry) => entry.error).length,
    diagnostics,
  };
  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
  if (result.p95Ms > 120) process.exitCode = 1;
  if (diagnostics.prepareCount !== 1) process.exitCode = 1;
  if (diagnostics.sourceBytes !== bytes.length) process.exitCode = 1;
} finally {
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
    sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * ratio) - 1)]
      .toFixed(2),
  );
}
