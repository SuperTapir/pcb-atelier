import { expect, test } from "./test";

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await expect(
    page.locator("header").getByText("双面非对称黄金卡", { exact: true }),
  ).toBeVisible();
});

test("P 聚焦项目媒体，输入框与模态窗口会抑制编辑器快捷键", async ({
  page,
}) => {
  await page.keyboard.press("p");

  const imageTool = page.getByRole("button", { name: "图片工具 (P)" });
  const search = page.getByRole("searchbox", { name: "搜索项目媒体" });
  await expect(imageTool).toHaveAttribute("aria-pressed", "true");
  await expect(search).toBeFocused();

  await page.keyboard.press("v");
  await expect(search).toHaveValue("v");
  await expect(imageTool).toHaveAttribute("aria-pressed", "true");

  await page.keyboard.press("Backspace");
  await expect(search).toHaveValue("");
  await expect(
    page
      .getByRole("tree", { name: "板体与生产层" })
      .getByRole("button", { name: "正面说明", exact: true }),
  ).toBeVisible();

  await page.getByRole("button", { name: "工程菜单" }).click();
  await page.getByRole("menuitem", { name: "设置…" }).click();
  await expect(page.getByRole("dialog", { name: "设置" })).toBeVisible();
  await page.keyboard.press("t");
  await expect(imageTool).toHaveAttribute("aria-pressed", "true");
});

test("方向键以 0.1 mm 微调，Shift 加速为 1 mm", async ({ page }) => {
  const tree = page.getByRole("tree", { name: "板体与生产层" });
  await tree
    .getByRole("button", { name: "正面说明", exact: true })
    .click();

  const x = page.getByRole("textbox", { name: "X (mm)" });
  const y = page.getByRole("textbox", { name: "Y (mm)" });
  const initialX = Number(await x.inputValue());
  const initialY = Number(await y.inputValue());

  await page.keyboard.press("ArrowRight");
  await expect(x).toHaveValue((initialX + 0.1).toFixed(3));

  await page.keyboard.press("Shift+ArrowDown");
  await expect(y).toHaveValue((initialY + 1).toFixed(3));
});
