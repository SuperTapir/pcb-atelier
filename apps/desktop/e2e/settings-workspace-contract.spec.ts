import { expect, test, type Page } from "./test";

test("全局偏好持久化但不改写工程，顶栏按整窗居中且 HUD 保持克制", async ({
  page,
}) => {
  await page.goto("/");
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  await expect(
    page.locator("header").getByText("双面非对称黄金卡", { exact: true }),
  ).toBeVisible();
  const before = await readWorkspaceDocument(page);

  await page.getByRole("button", { name: "工程菜单" }).click();
  await page.getByRole("menuitem", { name: "设置…" }).click();
  const settings = page.getByRole("dialog", { name: "设置" });
  await settings
    .getByRole("group", { name: "默认画板视图" })
    .getByRole("button", { name: "上下" })
    .click();
  await settings
    .getByRole("group", { name: "滚轮缩放速度" })
    .getByRole("button", { name: "慢" })
    .click();
  await settings
    .getByRole("group", { name: "启动窗口" })
    .getByRole("button", { name: "系统全屏" })
    .click();
  await settings
    .getByRole("group", { name: "界面外观" })
    .getByRole("button", { name: "深色" })
    .click();
  await page.getByRole("button", { name: "关闭设置" }).click();

  expect(await readWorkspaceDocument(page)).toEqual(before);
  const selector = page.getByRole("combobox", { name: "画板视图" });
  const selectorBox = await selector.boundingBox();
  const viewport = page.viewportSize();
  expect(selectorBox).not.toBeNull();
  expect(viewport).not.toBeNull();
  expect(
    Math.abs(selectorBox!.x + selectorBox!.width / 2 - viewport!.width / 2),
  ).toBeLessThanOrEqual(1);

  const front = page.getByTestId("workspace-canvas-front");
  await expect(front.getByText("正面", { exact: true })).toBeVisible();
  await expect(front.getByText(/^\d+%$/)).toBeVisible();
  await expect(front.getByText(/FRONT|拖动画布|X\s*[—-]\s*Y|mm/)).toHaveCount(0);

  await page.reload();
  await expect(selector).toHaveValue("vertical");
  await page.getByRole("button", { name: "工程菜单" }).click();
  await page.getByRole("menuitem", { name: "设置…" }).click();
  await expect(
    page
      .getByRole("dialog", { name: "设置" })
      .getByRole("group", { name: "滚轮缩放速度" })
      .getByRole("button", { name: "慢" }),
  ).toHaveAttribute("aria-pressed", "true");
});

async function readWorkspaceDocument(page: Page) {
  return page.evaluate(async () => {
    const response = await fetch("/__atelier_bridge", {
      body: JSON.stringify({
        args: {},
        command: "get_workspace_document",
        contractVersion: "pcb-atelier-workspace-v1",
      }),
      headers: { "Content-Type": "application/json" },
      method: "POST",
    });
    return (await response.json()) as unknown;
  });
}
