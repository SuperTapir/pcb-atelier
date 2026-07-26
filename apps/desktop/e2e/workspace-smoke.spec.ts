import { expect, test, type Locator } from "@playwright/test";
import path from "node:path";

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await expect(
    page.locator("header").getByText("双面非对称黄金卡", { exact: true }),
  ).toBeVisible();
});

test("编辑模式默认同时显示正反双画板，点击画板切换活动卡面", async ({
  page,
}) => {
  const front = page.getByTestId("workspace-canvas-front");
  const back = page.getByTestId("workspace-canvas-back");

  await expect(front).toBeVisible();
  await expect(back).toBeVisible();
  await expect(front).toHaveAttribute("data-edit-orientation", "upright");
  await expect(back).toHaveAttribute("data-edit-orientation", "upright");
  await expect(page.getByTestId("edit-board-layout")).toHaveAttribute(
    "data-layout",
    "both",
  );
  await expect(front).toHaveAttribute("data-active", "true");
  await expect(back).toHaveAttribute("data-active", "false");

  await clickCanvas(back);
  await expect(back).toHaveAttribute("data-active", "true");
  await expect(front).toHaveAttribute("data-active", "false");
  await expect(page.getByText("背面 · 编辑", { exact: true })).toBeVisible();
});

test("左侧只显示板体与正背生产层，不暴露独立内容入口", async ({ page }) => {
  const tree = page.getByRole("tree", { name: "板体与生产层" });
  await expect(tree.getByText("板体", { exact: true })).toBeVisible();
  await expect(tree.getByRole("group", { name: "正面" })).toBeVisible();
  await expect(tree.getByRole("group", { name: "背面" })).toBeVisible();
  await expect(tree.getByText("内容", { exact: true })).toHaveCount(0);
});

test("正反两面的选择状态在活动卡面切换后分别保留", async ({ page }) => {
  const tree = page.getByRole("tree", { name: "板体与生产层" });
  const frontSelection = tree
    .getByRole("treeitem")
    .filter({ hasText: "正面说明" });
  await frontSelection
    .getByRole("button", { name: "正面说明", exact: true })
    .click();
  await expect(frontSelection).toHaveAttribute("aria-selected", "true");

  await clickCanvas(page.getByTestId("workspace-canvas-back"));
  const backSelection = tree
    .getByRole("treeitem")
    .filter({ hasText: "背面说明" });
  await backSelection
    .getByRole("button", { name: "背面说明", exact: true })
    .click();
  await expect(backSelection).toHaveAttribute("aria-selected", "true");

  await clickCanvas(page.getByTestId("workspace-canvas-front"));
  await expect(
    tree.getByRole("treeitem").filter({ hasText: "正面说明" }),
  ).toHaveAttribute("aria-selected", "true");
  await expect(
    page.getByRole("heading", { name: "检查器" }).locator(".."),
  ).toContainText("正面说明");
});

test("对象图层可在左侧双击重命名并通过撤销恢复", async ({ page }) => {
  const tree = page.getByRole("tree", { name: "板体与生产层" });
  const source = tree
    .getByRole("treeitem")
    .filter({ hasText: "正面说明" });

  await source
    .getByRole("button", { name: "正面说明", exact: true })
    .dblclick();
  const editor = tree.getByRole("textbox", { name: "重命名 正面说明" });
  await expect(editor).toBeVisible();
  await editor.fill("正面副标题");
  await editor.press("Enter");

  await expect(
    tree.getByRole("button", { name: "正面副标题", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "撤销" }).click();
  await expect(
    tree.getByRole("button", { name: "正面说明", exact: true }),
  ).toBeVisible();
});

test("聚焦当前面只隐藏非活动画板，恢复同时查看后活动面不变", async ({
  page,
}) => {
  const layout = page.getByRole("group", { name: "画板布局" });
  await clickCanvas(page.getByTestId("workspace-canvas-back"));

  await layout.getByRole("button", { name: "聚焦当前面" }).click();
  await expect(page.getByTestId("workspace-canvas-front")).toHaveCount(0);
  await expect(page.getByTestId("workspace-canvas-back")).toBeVisible();
  await expect(page.getByTestId("workspace-canvas-back")).toHaveAttribute(
    "data-active",
    "true",
  );

  await layout.getByRole("button", { name: "同时查看" }).click();
  await expect(page.getByTestId("workspace-canvas-front")).toBeVisible();
  await expect(page.getByTestId("workspace-canvas-back")).toBeVisible();
  await expect(page.getByTestId("workspace-canvas-back")).toHaveAttribute(
    "data-active",
    "true",
  );
});

test("同时查看可强制左右或上下排列且不丢失活动卡面", async ({ page }) => {
  const arrangement = page.getByRole("group", { name: "画板排列" });
  const layout = page.getByTestId("edit-board-layout");
  await clickCanvas(page.getByTestId("workspace-canvas-back"));

  await arrangement.getByRole("button", { name: "左右" }).click();
  await expect(layout).toHaveAttribute("data-arrangement", "horizontal");
  await expect(page.getByTestId("workspace-canvas-front")).toBeVisible();
  await expect(page.getByTestId("workspace-canvas-back")).toHaveAttribute(
    "data-active",
    "true",
  );

  await arrangement.getByRole("button", { name: "上下" }).click();
  await expect(layout).toHaveAttribute("data-arrangement", "vertical");
  await expect(page.getByTestId("workspace-canvas-front")).toBeVisible();
  await expect(page.getByTestId("workspace-canvas-back")).toHaveAttribute(
    "data-active",
    "true",
  );
});

test("编辑与预览模式正交，预览显示 3D canvas，返回编辑恢复工具和活动面", async ({
  page,
}) => {
  const modes = page.getByRole("group", { name: "工作模式" });
  const textTool = page.getByRole("button", { name: "文字工具 (T)" });
  await clickCanvas(page.getByTestId("workspace-canvas-back"));
  await textTool.click();

  await modes.getByRole("button", { name: "预览" }).click();
  const preview = page.getByTestId("board-3d-preview");
  await expect(preview).toBeVisible({ timeout: 15_000 });
  await expect(preview.locator("canvas")).toBeVisible();
  await expect(page.getByTestId("edit-board-layout")).toHaveCount(0);

  await modes.getByRole("button", { name: "编辑" }).click();
  await expect(textTool).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByTestId("workspace-canvas-back")).toHaveAttribute(
    "data-active",
    "true",
  );
  await expect(page.getByTestId("workspace-canvas-front")).toBeVisible();
});

test("阻焊颜色进入真实编译纹理，切换预览保持流畅且画面发生变化", async ({
  page,
}) => {
  const modes = page.getByRole("group", { name: "工作模式" });
  const maskColor = page.getByRole("combobox", { name: "阻焊颜色" });

  await page.getByTestId("board-root").click();
  await maskColor.selectOption("black");
  await expect(page.getByRole("status")).toContainText("板体工艺参数已更新");
  await modes.getByRole("button", { name: "预览" }).click();

  const preview = page.getByTestId("board-3d-preview");
  await expect(preview).toBeVisible({ timeout: 5_000 });
  await expect(preview).toHaveAttribute("data-solder-mask-rgb", "27,29,28");
  const blackPreview = await preview.screenshot();

  await modes.getByRole("button", { name: "编辑" }).click();
  await page.getByTestId("board-root").click();
  await maskColor.selectOption("white");
  await expect(page.getByRole("status")).toContainText("板体工艺参数已更新");
  await modes.getByRole("button", { name: "预览" }).click();

  await expect(preview).toBeVisible({ timeout: 5_000 });
  await expect(preview).toHaveAttribute(
    "data-solder-mask-rgb",
    "226,228,222",
    { timeout: 5_000 },
  );
  await expect
    .poll(async () => (await preview.screenshot()).equals(blackPreview), {
      timeout: 5_000,
    })
    .toBe(false);
});

test("活动卡面决定插入目标，生产层上下文自动建立映射", async ({
  page,
}) => {
  const back = page.getByTestId("workspace-canvas-back");
  await clickCanvas(back);
  await page.getByTestId("production-context-back-silkscreen").click();
  await expect(
    page.getByTestId("production-context-back-silkscreen"),
  ).toHaveAttribute("aria-pressed", "true");

  await page.getByRole("button", { name: "文字工具 (T)" }).click();
  await clickCanvas(back);
  const editor = page.getByTestId("text-editor");
  await expect(editor).toBeVisible();
  await editor.fill("底面丝印");
  await editor.press("Escape");

  const tree = page.getByRole("tree", { name: "板体与生产层" });
  await expect(
    tree
      .getByRole("group", { name: "背面" })
      .getByText("文字", { exact: true }),
  ).toBeVisible();

  await page.getByRole("button", { name: "选择工具 (V)" }).click();
  await clickCanvas(page.getByTestId("workspace-canvas-front"));
  await expect(
    tree
      .getByRole("group", { name: "正面" })
      .getByText("文字", { exact: true }),
  ).toHaveCount(0);
});

test("文字检查器可以选择本机字体并用物理尺寸调整字号", async ({ page }) => {
  const tree = page.getByRole("tree", { name: "板体与生产层" });
  await tree
    .getByRole("treeitem")
    .filter({ hasText: "正面说明" })
    .getByRole("button", { name: "正面说明", exact: true })
    .click();

  const font = page.getByRole("combobox", { name: "字体" });
  const fontSize = page.getByRole("textbox", { name: "字号 (mm)" });
  await expect(font).toBeVisible();
  await expect(font.locator("option")).not.toHaveCount(1);

  const currentFamily = await font.inputValue();
  const options = await font.locator("option").evaluateAll((nodes) =>
    nodes.map((node) => (node as HTMLOptionElement).value),
  );
  const localFamily = options.find((family) => family !== currentFamily);
  expect(localFamily).toBeTruthy();
  await font.selectOption(localFamily!);
  await expect(page.getByRole("status")).toContainText("文字样式已更新");
  await expect(font).toHaveValue(localFamily!);

  const nextSize = (await fontSize.inputValue()) === "6.500" ? "5.500" : "6.500";
  await fontSize.fill(nextSize);
  await fontSize.press("Enter");
  await expect(page.getByRole("status")).toContainText("文字样式已更新");
  await expect(fontSize).toHaveValue(nextSize);
});

test("Web 文件输入插入真实图片并参与当前生产层与 3D 预览", async ({
  page,
}) => {
  await page.getByTestId("production-context-front-silkscreen").click();
  await page.getByTestId("image-file-input").setInputFiles(
    path.resolve(process.cwd(), "../../assets/branding/pcb-atelier-logo.png"),
  );
  await expect(page.getByRole("status")).toContainText("图片已插入并居中");

  const tree = page.getByRole("tree", { name: "板体与生产层" });
  await expect(
    tree
      .getByRole("group", { name: "正面" })
      .getByText("pcb-atelier-logo.png", { exact: true }),
  ).toBeVisible();

  await page
    .getByRole("group", { name: "工作模式" })
    .getByRole("button", { name: "预览" })
    .click();
  await expect(page.getByTestId("board-3d-preview")).toBeVisible({
    timeout: 15_000,
  });
  await expect(page.getByTestId("board-3d-preview").locator("canvas")).toBeVisible();
});

test("Web 左树多选可以通过共享服务完成分组与解组", async ({ page }) => {
  await page.getByTestId("production-context-back-silkscreen").click();
  const tree = page.getByRole("tree", { name: "板体与生产层" });
  const back = tree.getByRole("group", { name: "背面" });
  const mark = back.getByRole("treeitem").filter({ hasText: "背面标记" });
  const note = back.getByRole("treeitem").filter({ hasText: "背面说明" });

  await mark.getByRole("button", { name: "背面标记", exact: true }).click();
  await note
    .getByRole("button", { name: "背面说明", exact: true })
    .click({ modifiers: ["Shift"] });

  const groupButton = page.getByRole("button", { name: "分组", exact: true });
  await expect(groupButton).toBeEnabled();
  await groupButton.click();
  await expect(page.getByRole("status")).toContainText("已分组");
  const group = back.getByRole("treeitem").filter({ hasText: "组合" });
  await expect(group.getByText("组合", { exact: true })).toBeVisible();

  const groupX = page.getByRole("textbox", { name: "X (mm)" });
  await expect(groupX).toHaveValue("8.000");
  await groupX.fill("10");
  await groupX.press("Enter");
  await expect(page.getByRole("status")).toContainText("变换已更新");

  await group.getByRole("button", { name: "组合", exact: true }).click();
  await page.evaluate(() =>
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" })),
  );
  await expect(page.getByRole("status")).toContainText("已进入组合");
  await mark.getByRole("button", { name: "背面标记", exact: true }).click();
  await expect(groupX).toHaveValue("10.000");

  await page.evaluate(() =>
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })),
  );
  await expect(page.getByRole("status")).toContainText("已退出组合");
  await expect(group).toHaveAttribute("aria-selected", "true");

  const ungroupButton = page.getByRole("button", { name: "解组", exact: true });
  await expect(ungroupButton).toBeEnabled();
  await ungroupButton.click();
  await expect(page.getByRole("status")).toContainText("已解组");
  await expect(back.getByText("组合", { exact: true })).toHaveCount(0);
  await expect(back.getByText("背面标记", { exact: true })).toBeVisible();
  await expect(back.getByText("背面说明", { exact: true })).toBeVisible();
});

test("铜层工作上下文可以创建带板边间距的基础铺铜", async ({ page }) => {
  await page.getByTestId("production-context-front-copper").click();
  const tree = page.getByRole("tree", { name: "板体与生产层" });
  await tree.getByRole("button", { name: "添加基础铺铜" }).click();

  await expect(
    page.getByText("基础铺铜已选中，板边间距 0.50 mm", { exact: true }),
  ).toBeVisible();
  await expect(tree.getByText("基础铺铜", { exact: true })).toBeVisible();
  await tree.getByRole("button", { name: "添加基础铺铜" }).click();
  await expect(tree.getByText("基础铺铜", { exact: true })).toHaveCount(1);
  await expect(page.getByRole("button", { name: "撤销" })).toBeEnabled();
});

test("同一对象可关联多层且仍保持同一来源身份", async ({ page }) => {
  const tree = page.getByRole("tree", { name: "板体与生产层" });
  await page.getByTestId("production-context-back-silkscreen").click();
  const note = tree.getByRole("treeitem").filter({ hasText: "背面说明" });
  await note.getByRole("button", { name: "背面说明", exact: true }).click();

  const copper = page.getByRole("checkbox", { name: "铜层", exact: true });
  await copper.click();
  await expect(page.getByRole("status")).toContainText("生产层关联已更新");

  await page.getByTestId("production-context-back-copper").click();
  await expect(
    tree.getByRole("treeitem").filter({ hasText: "背面说明" }),
  ).toContainText("关联");

  await copper.click();
  await expect(page.getByRole("status")).toContainText("生产层关联已移除");
});

test("板体检查器修改尺寸不缩放对象并呈现越界警告", async ({ page }) => {
  const tree = page.getByRole("tree", { name: "板体与生产层" });
  await page.getByTestId("production-context-front-silkscreen").click();
  const note = tree.getByRole("treeitem").filter({ hasText: "正面说明" });
  await note.getByRole("button", { name: "正面说明", exact: true }).click();
  const x = page.getByRole("textbox", { name: "X (mm)" });
  const initialX = await x.inputValue();

  await page.getByTestId("board-root").click();
  const width = page.getByRole("textbox", { name: "宽 (mm)" });
  await width.fill("20.000");
  await width.press("Enter");
  await expect(page.getByRole("status")).toContainText(
    "对象物理尺寸保持不变",
  );
  await expect(page.getByText("越界警告", { exact: true })).toBeVisible();
  await expect(page.getByText("正面 · 正面说明", { exact: true })).toBeVisible();

  await page.getByTestId("production-context-front-silkscreen").click();
  await note.getByRole("button", { name: "正面说明", exact: true }).click();
  await expect(page.getByRole("textbox", { name: "X (mm)" })).toHaveValue(
    initialX,
  );

  await page.getByTestId("board-root").click();
  await width.fill("85.600");
  await width.press("Enter");
  await expect(page.getByText("越界警告", { exact: true })).toHaveCount(0);
});

async function clickCanvas(canvas: Locator) {
  await expect(canvas).toBeVisible();
  const box = await canvas.boundingBox();
  expect(box).not.toBeNull();
  await canvas.click({
    position: { x: box!.width / 2, y: box!.height / 2 },
  });
}
