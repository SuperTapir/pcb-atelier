import { spawn, spawnSync } from "node:child_process";
import { access } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { chromium } from "playwright-core";

const desktopDirectory = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
const port = Number(process.env.KONVA_BENCHMARK_PORT ?? 1422);
const url = `http://127.0.0.1:${port}/benchmark.html`;
const candidates = [
  process.env.CHROME_PATH,
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/Applications/Chromium.app/Contents/MacOS/Chromium",
  "/usr/bin/google-chrome",
  "/usr/bin/chromium",
  "/usr/bin/chromium-browser",
].filter(Boolean);

async function findChrome() {
  for (const candidate of candidates) {
    try {
      await access(candidate);
      return candidate;
    } catch {
      // Continue with the next conventional browser path.
    }
  }
  throw new Error(
    "未找到 Chrome/Chromium；请通过 CHROME_PATH 指向浏览器可执行文件。",
  );
}

async function waitForServer(timeoutMs = 20_000) {
  const startedAt = Date.now();
  while (Date.now() - startedAt < timeoutMs) {
    try {
      if ((await fetch(url)).ok) return;
    } catch {
      // Vite is still starting.
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`等待 Vite 超时：${url}`);
}

const viteEntry = path.join(
  desktopDirectory,
  "node_modules/vite/bin/vite.js",
);
const build = spawnSync(process.execPath, [viteEntry, "build"], {
  cwd: desktopDirectory,
  env: { ...process.env, NODE_ENV: "production" },
  stdio: "inherit",
});
if (build.status !== 0) process.exit(build.status ?? 1);

const server = spawn(
  process.execPath,
  [
    viteEntry,
    "preview",
    "--host",
    "127.0.0.1",
    "--port",
    String(port),
    "--strictPort",
  ],
  {
    cwd: desktopDirectory,
    env: { ...process.env, NODE_ENV: "production" },
    stdio: ["ignore", "pipe", "pipe"],
  },
);
let viteError = "";
server.stderr.on("data", (chunk) => {
  viteError += chunk.toString();
});

let browser;
try {
  await waitForServer();
  browser = await chromium.launch({
    executablePath: await findChrome(),
    headless: true,
    args: [
      "--enable-precise-memory-info",
      "--js-flags=--expose-gc",
      "--disable-background-timer-throttling",
      "--disable-renderer-backgrounding",
    ],
  });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  page.on("console", (message) => {
    if (message.type() === "error") {
      process.stderr.write(`[browser] ${message.text()}\n`);
    }
  });
  await page.goto(url, { waitUntil: "networkidle" });
  await page.waitForFunction(
    () => window.__KONVA_BENCHMARK__ !== undefined,
    null,
    { timeout: 60_000 },
  );
  const result = await page.evaluate(() => window.__KONVA_BENCHMARK__);
  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
  const dualBoardContractPassed =
    result?.version === 3 &&
    result.scene.boardCount === 2 &&
    result.scene.objectsPerFace === 100 &&
    result.scene.editableObjects === 200 &&
    result.gestures.panUpdates === 360 &&
    result.gestures.dragUpdates === 360 &&
    result.gestures.p95Ms <= 16.7 &&
    result.checks.inactiveBoard === true &&
    result.checks.noProductionCompile === true &&
    result.checks.noIpc === true &&
    result.checks.noSynchronousIpc === true;
  if (
    !result?.passed ||
    result.checks.heap !== true ||
    !dualBoardContractPassed
  ) {
    process.exitCode = 1;
  }
} catch (error) {
  process.stderr.write(`${error instanceof Error ? error.stack : error}\n`);
  if (viteError) process.stderr.write(`Vite stderr:\n${viteError}\n`);
  process.exitCode = 1;
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
