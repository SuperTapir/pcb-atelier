import { spawn, spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { mkdtemp, readFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const desktopDirectory = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
const workspaceDirectory = path.resolve(desktopDirectory, "../..");
const executable = path.join(
  workspaceDirectory,
  "target/release/konva-benchmark",
);
const runs = Number(process.env.KONVA_TAURI_RUNS ?? 3);
const resultTimeoutMs = 60_000;
const maximumRssCycleGrowthKib = 32 * 1024;
const maximumRssCycleGrowthRatio = 0.2;

function runChecked(command, args, cwd, extraEnvironment = {}) {
  const result = spawnSync(command, args, {
    cwd,
    env: {
      ...process.env,
      NODE_ENV: "production",
      ...extraEnvironment,
    },
    stdio: "inherit",
  });
  if (result.status !== 0) process.exit(result.status ?? 1);
}

function median(values) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.floor(sorted.length / 2)];
}

function commandOutput(command, args) {
  return spawnSync(command, args, { encoding: "utf8" }).stdout.trim();
}

async function waitForExit(child, timeoutMs) {
  return await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      child.kill("SIGTERM");
      reject(new Error(`Tauri benchmark timed out after ${timeoutMs} ms`));
    }, timeoutMs);
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      clearTimeout(timeout);
      if (code === 0) resolve();
      else reject(new Error(`Tauri exited with code=${code} signal=${signal}`));
    });
  });
}

async function readRssCheckpoints(resultPath) {
  const lines = (await readFile(`${resultPath}.rss.jsonl`, "utf8"))
    .trim()
    .split("\n")
    .filter(Boolean);
  return lines.map((line) => JSON.parse(line));
}

runChecked(process.execPath, ["node_modules/vite/bin/vite.js", "build"], desktopDirectory);
const benchmarkTauriConfig = readFileSync(
  path.join(desktopDirectory, "src-tauri/tauri.benchmark.conf.json"),
  "utf8",
);
runChecked(
  "cargo",
  [
    "build",
    "--release",
    "--bin",
    "konva-benchmark",
    "--features",
    "tauri/custom-protocol",
  ],
  workspaceDirectory,
  { TAURI_CONFIG: benchmarkTauriConfig },
);

const temporaryDirectory = await mkdtemp(
  path.join(os.tmpdir(), "pcb-atelier-konva-tauri-"),
);
const results = [];

for (let index = 0; index < runs; index += 1) {
  const resultPath = path.join(temporaryDirectory, `run-${index + 1}.json`);
  const startedAt = Date.now();
  const child = spawn(executable, [], {
    cwd: desktopDirectory,
    env: {
      ...process.env,
      PCB_ATELIER_BENCHMARK_RESULT: resultPath,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let stderr = "";
  child.stderr.on("data", (chunk) => {
    stderr += chunk.toString();
  });

  try {
    await waitForExit(child, resultTimeoutMs);
  } catch (error) {
    if (stderr) process.stderr.write(stderr);
    throw error;
  }

  const result = JSON.parse(await readFile(resultPath, "utf8"));
  const rssCheckpoints = await readRssCheckpoints(resultPath);
  const cycleCheckpoints = rssCheckpoints.filter(
    (checkpoint) =>
      checkpoint.cycle > 0 &&
      checkpoint.cycle <= result.scene.faceVisibilityCycles,
  );
  const firstCycleRss = cycleCheckpoints[0]?.rssKib ?? null;
  const lastCycleRss = cycleCheckpoints.at(-1)?.rssKib ?? null;
  const cycleGrowthKib =
    firstCycleRss === null || lastCycleRss === null
      ? null
      : lastCycleRss - firstCycleRss;
  const cycleGrowthRatio =
    cycleGrowthKib === null ? null : cycleGrowthKib / firstCycleRss;
  const rssPassed =
    cycleGrowthKib !== null &&
    cycleGrowthKib <= maximumRssCycleGrowthKib &&
    cycleGrowthRatio <= maximumRssCycleGrowthRatio;
  const dualBoardContractPassed =
    result.version === 2 &&
    result.scene.boardCount === 2 &&
    result.scene.objectsPerFace === 100 &&
    result.scene.editableObjects === 200;

  results.push({
    run: index + 1,
    wallTimeMs: Date.now() - startedAt,
    scene: result.scene,
    frames: result.frames,
    renderIsolation: result.renderIsolation,
    webviewHeap: result.memory,
    rss: {
      checkpoints: rssCheckpoints,
      cycleGrowthKib,
      cycleGrowthRatio,
      passed: rssPassed,
    },
    passed:
      result.checks.fps &&
      result.checks.p95Frame &&
      result.checks.slowFrameRatio &&
      result.checks.inactiveBoard &&
      dualBoardContractPassed &&
      rssPassed,
  });
}

const report = {
  version: 2,
  recordedAt: new Date().toISOString(),
  environment: {
    platform: process.platform,
    architecture: process.arch,
    osVersion:
      process.platform === "darwin"
        ? commandOutput("sw_vers", ["-productVersion"])
        : os.release(),
    hardware:
      process.platform === "darwin"
        ? commandOutput("sysctl", ["-n", "hw.model"])
        : os.hostname(),
    runs,
    window: { width: 1280, height: 800 },
  },
  thresholds: {
    minimumFps: 45,
    maximumP95FrameMs: 1000 / 30,
    slowFrameBoundaryMs: 1000 / 45,
    maximumSlowFrameRatio: 0.2,
    maximumInactiveDrawsPerActiveCycle: 6,
    maximumInactiveDrawRatio: 0.02,
    maximumRssCycleGrowthKib,
    maximumRssCycleGrowthRatio,
  },
  results,
  median: {
    fps: median(results.map((result) => result.frames.fps)),
    averageFrameMs: median(
      results.map((result) => result.frames.averageMs),
    ),
    p95FrameMs: median(results.map((result) => result.frames.p95Ms)),
    maximumFrameMs: median(
      results.map((result) => result.frames.maximumMs),
    ),
    slowFrameRatio: median(
      results.map((result) => result.frames.slowFrameRatio),
    ),
    inactiveDraws: median(
      results.map(
        (result) => result.renderIsolation.totalInactiveDraws,
      ),
    ),
    inactiveDrawRatio: median(
      results.map(
        (result) => result.renderIsolation.inactiveDrawRatio,
      ),
    ),
    rssCycleGrowthKib: median(
      results.map((result) => result.rss.cycleGrowthKib),
    ),
    rssCycleGrowthRatio: median(
      results.map((result) => result.rss.cycleGrowthRatio),
    ),
  },
  passed: results.every((result) => result.passed),
  caveat:
    "WKWebView does not expose performance.memory. RSS checkpoints sample the benchmark host process and are a coarse leak guard; retained JavaScript heap still requires Web Inspector or Instruments for release sign-off.",
};

process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
if (!report.passed) process.exitCode = 1;
