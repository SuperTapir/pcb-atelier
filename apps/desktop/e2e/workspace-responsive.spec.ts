import { expect, test, type Page } from "./test";

for (const viewport of [
  { width: 1440, height: 900 },
  { width: 1920, height: 1080 },
]) {
  test(`${viewport.width}px 同时显示生产层、媒体库、双画布和检查器`, async ({
    page,
  }) => {
    await openFreshWorkspace(page, viewport);

    await expect(page.getByRole("region", { name: "生产层" })).toBeVisible();
    await expect(page.getByRole("region", { name: "项目媒体" })).toBeVisible();
    await expect(page.getByTestId("workspace-canvas-front")).toBeVisible();
    await expect(page.getByTestId("workspace-canvas-back")).toBeVisible();
    await expect(page.getByRole("heading", { name: "检查器" })).toBeVisible();
    await expect(page.getByTestId("workspace-canvas-front")).toHaveCSS(
      "user-select",
      "none",
    );

    const [front, back] = await Promise.all([
      page.getByTestId("workspace-canvas-front").boundingBox(),
      page.getByTestId("workspace-canvas-back").boundingBox(),
    ]);
    expect(front).not.toBeNull();
    expect(back).not.toBeNull();
    expect(front!.x + front!.width).toBeLessThanOrEqual(back!.x);
    await expectNoWindowOverflow(page);
  });
}

test("960px 降级为无横向溢出的双画布，并保留媒体折叠状态", async ({
  page,
}) => {
  await openFreshWorkspace(page, { width: 960, height: 720 });

  const layout = page.getByTestId("edit-board-layout");
  const [front, back] = await Promise.all([
    page.getByTestId("workspace-canvas-front").boundingBox(),
    page.getByTestId("workspace-canvas-back").boundingBox(),
  ]);
  expect(front).not.toBeNull();
  expect(back).not.toBeNull();
  expect(Math.abs(front!.x - back!.x)).toBeLessThanOrEqual(1);
  expect(back!.y).toBeGreaterThanOrEqual(front!.y + front!.height);
  expect(
    await layout.evaluate((element) => element.scrollWidth <= element.clientWidth),
  ).toBe(true);
  await expect(page.getByTestId("active-board-divider")).toBeHidden();
  await expectNoWindowOverflow(page);

  const search = page.getByRole("searchbox", { name: "搜索项目媒体" });
  await search.fill("保留状态");
  await page.getByRole("button", { name: "折叠项目媒体" }).click();
  await expect(page.getByRole("button", { name: "展开项目媒体" })).toBeVisible();

  await page.reload();
  await expect(page.getByRole("button", { name: "展开项目媒体" })).toBeVisible();
  await page.getByRole("button", { name: "展开项目媒体" }).click();
  await expect(page.getByRole("searchbox", { name: "搜索项目媒体" })).toHaveValue(
    "保留状态",
  );
  await expect(page.getByRole("region", { name: "生产层" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "检查器" })).toBeVisible();
});

test("左右侧栏可在限制内拖动并跨刷新恢复宽度", async ({ page }) => {
  await openFreshWorkspace(page, { width: 1440, height: 900 });

  const leftHandle = page.getByTestId("workspace-left-panel-resizer");
  const rightHandle = page.getByTestId("workspace-right-panel-resizer");
  const leftBefore = await leftHandle.boundingBox();
  const rightBefore = await rightHandle.boundingBox();
  expect(leftBefore).not.toBeNull();
  expect(rightBefore).not.toBeNull();

  await page.mouse.move(
    leftBefore!.x + leftBefore!.width / 2,
    leftBefore!.y + 120,
  );
  await page.mouse.down();
  await page.mouse.move(leftBefore!.x + 100, leftBefore!.y + 120);
  await page.mouse.up();

  await page.mouse.move(
    rightBefore!.x + rightBefore!.width / 2,
    rightBefore!.y + 120,
  );
  await page.mouse.down();
  await page.mouse.move(rightBefore!.x - 80, rightBefore!.y + 120);
  await page.mouse.up();

  await expect(leftHandle).toHaveAttribute("aria-valuenow", "307");
  await expect(rightHandle).toHaveAttribute("aria-valuenow", "383");
  const persisted = await page.evaluate(() =>
    JSON.parse(localStorage.getItem("pcb-atelier.app-settings.v2") ?? "{}"),
  );
  expect(persisted.workspaceLeftPanelWidth).toBe(307);
  expect(persisted.workspaceRightPanelWidth).toBe(383);

  await page.reload();
  await expect(leftHandle).toHaveAttribute("aria-valuenow", "307");
  await expect(rightHandle).toHaveAttribute("aria-valuenow", "383");
  await expectNoWindowOverflow(page);
});

async function openFreshWorkspace(
  page: Page,
  viewport: { width: number; height: number },
) {
  await page.setViewportSize(viewport);
  await page.goto("/");
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  await expect(
    page.locator("header").getByText("双面非对称黄金卡", { exact: true }),
  ).toBeVisible();
}

async function expectNoWindowOverflow(page: Page) {
  expect(
    await page.evaluate(
      () =>
        document.documentElement.scrollWidth <= window.innerWidth &&
        document.body.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
}
