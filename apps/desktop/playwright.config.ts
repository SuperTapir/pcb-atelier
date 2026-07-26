import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  outputDir: "./test-results/playwright",
  fullyParallel: false,
  workers: 1,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 1 : 0,
  reporter: [["list"]],
  use: {
    baseURL: "http://127.0.0.1:1423",
    channel: "chrome",
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
    viewport: { width: 1280, height: 800 },
  },
  webServer: [
    {
      command: "npm run dev:e2e",
      url: "http://127.0.0.1:1423",
      reuseExistingServer: !process.env.CI,
      timeout: 30_000,
    },
    {
      command: "cargo run -p atelier-desktop --bin workspace-bridge",
      url: "http://127.0.0.1:1424/health",
      reuseExistingServer: !process.env.CI,
      timeout: 60_000,
    },
  ],
});
