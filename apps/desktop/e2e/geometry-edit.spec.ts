import { expect, test, type Locator } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await expect(
    page.locator("header").getByText("双面非对称黄金卡", { exact: true }),
  ).toBeVisible();
});

test("检查器提交精确毫米值，方向键按 0.1 mm 或 1 mm 微调", async ({ page }) => {
  await selectLayer(
    page.getByRole("tree", { name: "板体与生产层" }),
    "正面说明",
  );

  const x = page.getByRole("textbox", { name: "X (mm)" });
  const y = page.getByRole("textbox", { name: "Y (mm)" });
  await x.fill("12.345");
  await x.press("Enter");
  await expect(x).toHaveValue("12.345");

  await page.keyboard.press("ArrowRight");
  await expect(x).toHaveValue("12.445");

  await page.keyboard.press("Shift+ArrowDown");
  await expect(y).toHaveValue("9.000");
  await expect(page.getByText("变换已更新", { exact: true })).toBeVisible();
});

test("无效尺寸回退，祖先锁定禁用检查器并阻止键盘变换", async ({ page }) => {
  const tree = page.getByRole("tree", { name: "板体与生产层" });
  await selectLayer(tree, "正面说明");
  const width = page.getByRole("textbox", { name: "宽 (mm)" });
  await width.fill("-1");
  await width.press("Enter");
  await expect(width).toHaveValue("24.000");
  await expect(page.getByRole("status")).toContainText("尺寸必须大于");

  const group = tree.getByRole("treeitem").filter({ hasText: "正面组合" });
  await group
    .getByRole("button", { name: "正面组合", exact: true })
    .click();
  await group.getByRole("button", { name: "锁定对象" }).click();
  await selectLayer(tree, "正面标题");

  const x = page.getByRole("textbox", { name: "X (mm)" });
  await expect(x).toBeDisabled();
  const before = await x.inputValue();
  await page.keyboard.press("ArrowRight");
  await expect(x).toHaveValue(before);
});

test("画布拖动显示吸附 guide，按住 Alt 临时绕过", async ({ page }) => {
  const tree = page.getByRole("tree", { name: "板体与生产层" });
  await selectLayer(tree, "正面说明");
  const xInput = page.getByRole("textbox", { name: "X (mm)" });
  const yInput = page.getByRole("textbox", { name: "Y (mm)" });
  await xInput.fill("12.445");
  await xInput.press("Enter");
  await yInput.fill("20.000");
  await yInput.press("Enter");
  await expect(page.getByText("变换已更新", { exact: true })).toBeVisible();

  const canvas = page.getByTestId("workspace-canvas-front");
  const box = await canvas.boundingBox();
  expect(box).not.toBeNull();
  const boardLeft = box!.x + (box!.width - 85.6 * 5.5) / 2;
  const boardTop = box!.y + (box!.height - 53.98 * 5.5) / 2;
  const startX = boardLeft + (Number(await xInput.inputValue()) + 2) * 5.5;
  const startY = boardTop + (Number(await yInput.inputValue()) + 2) * 5.5;

  await page.mouse.move(startX, startY);
  await page.mouse.down();
  await page.mouse.move(startX + 4, startY, { steps: 4 });
  await expect(page.getByTestId("snap-guides-front")).toContainText(
    "X 对齐到 25.000 mm 网格",
  );
  await page.mouse.up();
  await expect(page.getByTestId("snap-guides-front")).toHaveCount(0);
  await expect(xInput).toHaveValue("13.000");

  await xInput.fill("12.445");
  await xInput.press("Enter");
  await expect(page.getByText("变换已更新", { exact: true })).toBeVisible();
  await page.keyboard.down("Alt");
  await page.mouse.move(startX, startY);
  await page.mouse.down();
  await page.mouse.move(startX + 4, startY, { steps: 4 });
  await expect(page.getByTestId("snap-guides-front")).toHaveCount(0);
  await page.mouse.up();
  await page.keyboard.up("Alt");
  await expect(xInput).not.toHaveValue("13.000");
});

async function selectLayer(tree: Locator, name: string) {
  const item = tree.getByRole("treeitem").filter({ hasText: name });
  await item.getByRole("button", { name, exact: true }).click();
  await expect(item).toHaveAttribute("aria-selected", "true");
}
