import { test as base } from "@playwright/test";

export { expect } from "@playwright/test";
export type { Locator, Page } from "@playwright/test";

export const test = base.extend<{ resetDevelopmentWorkspace: void }>({
  resetDevelopmentWorkspace: [
    async ({ request }, use) => {
      const address =
        process.env.PCB_ATELIER_BRIDGE_ADDR ?? "127.0.0.1:1424";
      const response = await request.post(`http://${address}/reset`);
      if (!response.ok()) {
        throw new Error(
          `failed to reset development workspace: HTTP ${response.status()}`,
        );
      }
      await use();
    },
    { auto: true },
  ],
});
