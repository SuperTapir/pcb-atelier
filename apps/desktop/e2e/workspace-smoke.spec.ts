import { expect, test, type Locator, type Page } from "./test";
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

  await page.getByRole("button", { name: "工程菜单" }).click();
  await expect(
    page.getByRole("menuitem", { name: "打开工程…" }),
  ).toBeVisible();
  await expect(page.getByRole("menuitem", { name: "保存 ⌘S" })).toBeVisible();
  await page.getByRole("button", { name: "工程菜单" }).click();
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

test("项目媒体工具条不会越过左侧 Dock 边界", async ({ page }) => {
  const media = page.getByRole("region", { name: "项目媒体" });
  const listButton = media.getByRole("button", { name: "列表视图" });
  const [mediaBox, buttonBox] = await Promise.all([
    media.boundingBox(),
    listButton.boundingBox(),
  ]);
  expect(mediaBox).not.toBeNull();
  expect(buttonBox).not.toBeNull();
  expect(buttonBox!.x + buttonBox!.width).toBeLessThanOrEqual(
    mediaBox!.x + mediaBox!.width,
  );
});

test("大屏适配画布会利用可用空间而不是锁在 100%", async ({ page }) => {
  await page.setViewportSize({ width: 1920, height: 1080 });
  await page.getByRole("button", { name: "适配画布" }).click();

  const zoomText = await page
    .getByTestId("workspace-canvas-front")
    .getByText(/^\d+%$/)
    .textContent();
  expect(Number.parseInt(zoomText ?? "0", 10)).toBeGreaterThan(100);
});

test("生产层可以同时展开，展开状态与当前编辑焦点彼此独立", async ({
  page,
}) => {
  await page.getByTestId("production-context-front-copper").click();
  await page.getByTestId("production-context-front-solderMaskOpen").click();

  await expect(
    page.getByRole("button", { name: "收起正面铜层" }),
  ).toHaveAttribute("aria-expanded", "true");
  await expect(
    page.getByRole("button", { name: "收起正面阻焊开窗" }),
  ).toHaveAttribute("aria-expanded", "true");
  await expect(
    page.getByRole("button", { name: "收起正面丝印层" }),
  ).toHaveAttribute("aria-expanded", "true");
  await expect(
    page.getByTestId("production-context-front-solderMaskOpen"),
  ).toHaveAttribute("aria-pressed", "true");

  await page.getByRole("button", { name: "收起正面铜层" }).click();
  await expect(
    page.getByRole("button", { name: "展开正面铜层" }),
  ).toHaveAttribute("aria-expanded", "false");
  await expect(
    page.getByTestId("production-context-front-solderMaskOpen"),
  ).toHaveAttribute("aria-pressed", "true");
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

test("多选对象可作为一个选区拖动并通过一次撤销整体恢复", async ({ page }) => {
  await clickCanvas(page.getByTestId("workspace-canvas-back"));
  const tree = page.getByRole("tree", { name: "板体与生产层" });
  const firstRow = tree
    .getByRole("treeitem")
    .filter({ hasText: "背面标记" });
  const secondRow = tree
    .getByRole("treeitem")
    .filter({ hasText: "背面说明" });
  await firstRow
    .getByRole("button", { name: "背面标记", exact: true })
    .click();
  await secondRow
    .getByRole("button", { name: "背面说明", exact: true })
    .click({ modifiers: ["Shift"] });
  await expect(firstRow).toHaveAttribute("aria-selected", "true");
  await expect(secondRow).toHaveAttribute("aria-selected", "true");

  const before = await readWorkspaceDocument(page);
  const first = before.backLayers.find((layer) => layer.name === "背面标记");
  const second = before.backLayers.find((layer) => layer.name === "背面说明");
  if (!first || !second) throw new Error("缺少多选拖拽测试对象");
  const canvas = page.getByTestId("workspace-canvas-back");
  const bounds = await canvas.boundingBox();
  if (!bounds) throw new Error("背面画板不可见");
  const viewport = await canvas.evaluate((element) => ({
    panX: Number((element as HTMLElement).dataset.viewportPanX),
    panY: Number((element as HTMLElement).dataset.viewportPanY),
    zoom: Number((element as HTMLElement).dataset.viewportZoom),
  }));
  const scale = 5.5 * viewport.zoom;
  const originX =
    bounds.width / 2 -
    (before.board.widthUm / 1_000) * scale / 2 +
    viewport.panX;
  const originY =
    bounds.height / 2 -
    (before.board.heightUm / 1_000) * scale / 2 +
    viewport.panY;
  const source = {
    x:
      bounds.x +
      originX +
      (first.transform.xUm + first.transform.widthUm / 2) / 1_000 * scale,
    y:
      bounds.y +
      originY +
      (first.transform.yUm + first.transform.heightUm / 2) / 1_000 * scale,
  };
  await page.mouse.move(source.x, source.y);
  await page.mouse.down();
  await page.mouse.move(source.x + 36, source.y + 18, { steps: 6 });
  await page.mouse.up();
  await expect(page.getByRole("status")).toContainText("已移动");

  const after = await readWorkspaceDocument(page);
  const movedFirst = after.backLayers.find((layer) => layer.id === first.id);
  const movedSecond = after.backLayers.find((layer) => layer.id === second.id);
  if (!movedFirst || !movedSecond) throw new Error("拖拽后对象丢失");
  const delta = {
    x: movedFirst.transform.xUm - first.transform.xUm,
    y: movedFirst.transform.yUm - first.transform.yUm,
  };
  expect(delta.x || delta.y).not.toBe(0);
  expect(movedSecond.transform.xUm - second.transform.xUm).toBe(delta.x);
  expect(movedSecond.transform.yUm - second.transform.yUm).toBe(delta.y);

  await page.keyboard.press("Meta+z");
  await expect
    .poll(async () => {
      const restored = await readWorkspaceDocument(page);
      return restored.backLayers
        .filter((layer) => layer.id === first.id || layer.id === second.id)
        .map((layer) => layer.transform);
    })
    .toEqual([first.transform, second.transform]);
});

test("多选删除先说明映射影响并可通过一次撤销整体恢复", async ({ page }) => {
  const tree = page.getByRole("tree", { name: "板体与生产层" });
  const first = tree.getByRole("treeitem").filter({ hasText: "背面标记" });
  const second = tree.getByRole("treeitem").filter({ hasText: "背面说明" });
  await first
    .getByRole("button", { name: "背面标记", exact: true })
    .click();
  await second
    .getByRole("button", { name: "背面说明", exact: true })
    .click({ modifiers: ["Shift"] });
  await page.keyboard.press("Delete");

  const confirmation = page.getByRole("dialog", { name: "确认删除对象" });
  await expect(confirmation).toContainText("2 个对象及 2 条生产层映射");
  await expect(confirmation).toContainText("素材与处理版本会保留");
  await confirmation.getByRole("button", { name: "删除对象" }).click();
  await expect(first).toHaveCount(0);
  await expect(second).toHaveCount(0);

  await page.keyboard.press("Meta+z");
  await expect(
    tree.getByRole("treeitem").filter({ hasText: "背面标记" }),
  ).toBeVisible();
  await expect(
    tree.getByRole("treeitem").filter({ hasText: "背面说明" }),
  ).toBeVisible();
});

test("生产层默认全部显示，点击画板空白取消选择框", async ({
  page,
}) => {
  const tree = page.getByRole("tree", { name: "板体与生产层" });
  const frontTitle = tree
    .getByRole("treeitem")
    .filter({ hasText: "正面说明" });
  await expect(
    tree.getByRole("button", { name: "隐藏正面铜层", exact: true }),
  ).toBeVisible();

  await frontTitle
    .getByRole("button", { name: "正面说明", exact: true })
    .click();
  await expect(frontTitle).toHaveAttribute("aria-selected", "true");

  const canvas = page.getByTestId("workspace-canvas-front");
  const bounds = await canvas.boundingBox();
  if (!bounds) throw new Error("正面画板不可见");
  await canvas.click({ position: { x: 16, y: bounds.height - 16 } });
  await expect(frontTitle).toHaveAttribute("aria-selected", "false");
});

test("对象图层可在左侧按 Return 重命名并通过撤销恢复", async ({ page }) => {
  const tree = page.getByRole("tree", { name: "板体与生产层" });
  const source = tree
    .getByRole("treeitem")
    .filter({ hasText: "正面说明" });

  const layerName = source.getByRole("button", {
    name: "正面说明",
    exact: true,
  });
  await layerName.click();
  await layerName.press("Enter");
  const editor = tree.getByRole("textbox", { name: "重命名 正面说明" });
  await expect(editor).toBeVisible();
  await editor.fill("正面副标题");
  await editor.press("Enter");

  await expect(
    tree.getByRole("button", { name: "正面副标题", exact: true }),
  ).toBeVisible();
  await page.keyboard.press("Meta+z");
  await expect(
    tree.getByRole("button", { name: "正面说明", exact: true }),
  ).toBeVisible();
});

test("对象行支持右键菜单与同级拖拽排序，不再提供上下移动按钮", async ({
  page,
}) => {
  const tree = page.getByRole("tree", { name: "板体与生产层" });
  const title = tree
    .getByRole("treeitem")
    .filter({ hasText: "背面标记" });
  const caption = tree
    .getByRole("treeitem")
    .filter({ hasText: "背面说明" });

  await title
    .getByRole("button", { name: "背面标记", exact: true })
    .click({ button: "right" });
  await expect(page.getByRole("menuitem", { name: "重命名" })).toBeVisible();
  await expect(page.getByRole("menuitem", { name: "上移" })).toHaveCount(0);
  await expect(page.getByRole("menuitem", { name: "下移" })).toHaveCount(0);

  await page.getByTestId("board-root").click();
  await expect(page.getByRole("button", { name: "重命名" })).toBeHidden();
  await title
    .getByRole("button", { name: "背面标记", exact: true })
    .click({ button: "right" });
  await page.keyboard.press("Escape");
  await expect(page.getByRole("button", { name: "重命名" })).toBeHidden();

  await title.dragTo(caption);
  await expect(page.getByRole("status")).toContainText("图层层级与顺序已更新");
});

test("聚焦当前面只隐藏非活动画板，恢复同时查看后活动面不变", async ({
  page,
}) => {
  const layout = page.getByRole("combobox", { name: "画板视图" });
  await clickCanvas(page.getByTestId("workspace-canvas-back"));

  await layout.selectOption("focus-active");
  await expect(page.getByTestId("workspace-canvas-front")).toHaveCount(0);
  await expect(page.getByTestId("workspace-canvas-back")).toBeVisible();
  await expect(page.getByTestId("workspace-canvas-back")).toHaveAttribute(
    "data-active",
    "true",
  );

  await layout.selectOption("horizontal");
  await expect(page.getByTestId("workspace-canvas-front")).toBeVisible();
  await expect(page.getByTestId("workspace-canvas-back")).toBeVisible();
  await expect(page.getByTestId("workspace-canvas-back")).toHaveAttribute(
    "data-active",
    "true",
  );
});

test("同时查看可强制左右或上下排列且不丢失活动卡面", async ({ page }) => {
  const arrangement = page.getByRole("combobox", { name: "画板视图" });
  const layout = page.getByTestId("edit-board-layout");
  await clickCanvas(page.getByTestId("workspace-canvas-back"));

  await expect(arrangement.getByRole("option", { name: /自动/ })).toHaveCount(0);
  await arrangement.selectOption("horizontal");
  await expect(layout).toHaveAttribute("data-arrangement", "horizontal");
  await expect(page.getByTestId("active-board-divider")).toHaveAttribute(
    "data-orientation",
    "vertical",
  );
  await expect(page.getByTestId("workspace-canvas-front")).toBeVisible();
  await expect(page.getByTestId("workspace-canvas-back")).toHaveAttribute(
    "data-active",
    "true",
  );
  const horizontalZoom = await page
    .getByTestId("workspace-canvas-front")
    .getByText(/^\d+%$/)
    .textContent();

  await arrangement.selectOption("vertical");
  await expect(layout).toHaveAttribute("data-arrangement", "vertical");
  await expect(page.getByTestId("active-board-divider")).toHaveAttribute(
    "data-orientation",
    "horizontal",
  );
  await expect(page.getByTestId("workspace-canvas-front")).toBeVisible();
  await expect(page.getByTestId("workspace-canvas-back")).toHaveAttribute(
    "data-active",
    "true",
  );
  await expect
    .poll(() =>
      page
        .getByTestId("workspace-canvas-front")
        .getByText(/^\d+%$/)
        .textContent(),
    )
    .not.toBe(horizontalZoom);
});

test("默认左右排列，设置页与工具栏选择在重载后保持一致", async ({ page }) => {
  const layout = page.getByTestId("edit-board-layout");
  await expect(layout).toHaveAttribute("data-arrangement", "horizontal");

  await page.getByRole("button", { name: "工程菜单" }).click();
  await page.getByRole("menuitem", { name: "设置…" }).click();
  const settings = page.getByRole("dialog", { name: "设置" });
  await expect(settings).toBeVisible();
  await settings
    .getByRole("group", { name: "默认画板视图" })
    .getByRole("button", { name: "上下" })
    .click();
  await expect(layout).toHaveAttribute("data-arrangement", "vertical");
  await expect(layout).toHaveAttribute("data-layout", "both");
  await settings
    .getByRole("group", { name: "默认画板视图" })
    .getByRole("button", { name: "聚焦当前面" })
    .click();
  await expect(layout).toHaveAttribute("data-layout", "focus");
  await page.getByRole("button", { name: "关闭设置" }).click();

  await page.reload();
  await expect(
    page.locator("header").getByText("双面非对称黄金卡", { exact: true }),
  ).toBeVisible();
  await expect(page.getByTestId("edit-board-layout")).toHaveAttribute(
    "data-layout",
    "focus",
  );
  await expect(page.getByTestId("workspace-canvas-back")).toHaveCount(0);
});

test("设置页用单一紧凑选项表达默认画板视图", async ({
  page,
}) => {
  await page.getByRole("button", { name: "工程菜单" }).click();
  await page.getByRole("menuitem", { name: "设置…" }).click();
  const settings = page.getByRole("dialog", { name: "设置" });
  const canvasView = settings.getByRole("group", { name: "默认画板视图" });
  const zoomSpeed = settings.getByRole("group", { name: "滚轮缩放速度" });

  await expect(canvasView.getByRole("button", { name: "左右" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(canvasView.getByRole("button", { name: "上下" })).toBeVisible();
  await expect(zoomSpeed.getByRole("button", { name: "标准" })).toBeVisible();
  const launchWindow = settings.getByRole("group", { name: "启动窗口" });
  await expect(
    launchWindow.getByRole("button", { name: "窗口化全屏" }),
  ).toHaveAttribute("aria-pressed", "true");
  await launchWindow.getByRole("button", { name: "系统全屏" }).click();
  await expect(
    launchWindow.getByRole("button", { name: "系统全屏" }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect
    .poll(() =>
      page.evaluate(() =>
        localStorage.getItem("pcb-atelier.app-settings.v2"),
      ),
    )
    .toContain('"launchWindowMode":"fullscreen"');
  await canvasView.getByRole("button", { name: "聚焦当前面" }).click();
  await expect(
    canvasView.getByRole("button", { name: "聚焦当前面" }),
  ).toHaveAttribute("aria-pressed", "true");

  const dialogBox = await settings.boundingBox();
  expect(dialogBox).not.toBeNull();
  expect(dialogBox!.y).toBeGreaterThanOrEqual(16);
  expect(dialogBox!.y + dialogBox!.height).toBeLessThanOrEqual(784);
});

test("设置页可使用 Escape 关闭", async ({ page }) => {
  await page.getByRole("button", { name: "工程菜单" }).click();
  await page.getByRole("menuitem", { name: "设置…" }).click();
  await expect(page.getByRole("dialog", { name: "设置" })).toBeVisible();

  await page.keyboard.press("Escape");

  await expect(page.getByRole("dialog", { name: "设置" })).toHaveCount(0);
});

test("工程菜单和设置面板使用不透明浮层背景", async ({ page }) => {
  await page.getByRole("button", { name: "工程菜单" }).click();
  const projectMenu = page.getByRole("menu", { name: "工程操作" });
  await expect(projectMenu).not.toHaveCSS(
    "background-color",
    "rgba(0, 0, 0, 0)",
  );

  await page.getByRole("menuitem", { name: "设置…" }).click();
  await expect(page.getByRole("dialog", { name: "设置" })).not.toHaveCSS(
    "background-color",
    "rgba(0, 0, 0, 0)",
  );
});

test("编辑与预览模式正交，预览显示 3D canvas，返回编辑恢复工具和活动面", async ({
  page,
}) => {
  const modes = page.getByRole("group", { name: "工作模式" });
  const editMode = modes.getByRole("button", { name: "编辑" });
  const modePositionBefore = await editMode.boundingBox();
  const textTool = page.getByRole("button", { name: "文字工具 (T)" });
  await clickCanvas(page.getByTestId("workspace-canvas-back"));
  await textTool.click();

  await modes.getByRole("button", { name: "预览" }).click();
  const modePositionAfter = await editMode.boundingBox();
  expect(modePositionAfter?.x).toBe(modePositionBefore?.x);
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

test("普通编辑只显示源对象，不在画布下重复编译生产纹理", async ({
  page,
}) => {
  const bridgeCommands: string[] = [];
  page.on("request", (request) => {
    if (!request.url().endsWith("/__atelier_bridge")) return;
    const payload = request.postDataJSON() as { command?: string } | null;
    if (payload?.command) bridgeCommands.push(payload.command);
  });

  await page.reload();
  await expect(
    page.locator("header").getByText("双面非对称黄金卡", { exact: true }),
  ).toBeVisible();
  await page.waitForTimeout(400);

  expect(bridgeCommands).not.toContain("get_production_preview");
  await expect(page.getByTestId("workspace-canvas-front")).toBeVisible();
  await expect(page.getByTestId("workspace-canvas-back")).toBeVisible();
});

test("阻焊颜色进入真实编译纹理，切换预览保持流畅且画面发生变化", async ({
  page,
}) => {
  const modes = page.getByRole("group", { name: "工作模式" });
  const maskColor = page.getByRole("combobox", { name: "阻焊油墨" });

  await page.getByTestId("board-root").click();
  await maskColor.selectOption("black");
  await expect(page.getByRole("status")).toContainText("制造参数与工艺近似已更新");
  await modes.getByRole("button", { name: "预览" }).click();

  const preview = page.getByTestId("board-3d-preview");
  await expect(preview).toBeVisible({ timeout: 5_000 });
  await expect(preview).toHaveAttribute("data-solder-mask-rgb", "27,29,28");
  const blackPreview = await preview.screenshot();

  await modes.getByRole("button", { name: "编辑" }).click();
  await page.getByTestId("board-root").click();
  await maskColor.selectOption("white");
  await expect(page.getByRole("status")).toContainText("制造参数与工艺近似已更新");
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
  await expect(
    page.getByRole("button", { name: "选择工具 (V)" }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(
    page.getByRole("button", { name: "文字工具 (T)" }),
  ).toHaveAttribute("aria-pressed", "false");
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
  const previewRequests: unknown[] = [];
  const previewResponses: Array<{ error: string | null; revision?: number }> = [];
  page.on("request", (request) => {
    if (!request.url().endsWith("/__atelier_bridge")) return;
    const payload = request.postDataJSON() as {
      command?: string;
      args?: { request?: unknown };
    } | null;
    if (payload?.command === "preview_image_import") {
      previewRequests.push(payload.args?.request);
    }
  });
  page.on("response", async (response) => {
    const request = response.request();
    if (!request.url().endsWith("/__atelier_bridge")) return;
    const payload = request.postDataJSON() as { command?: string } | null;
    if (payload?.command !== "preview_image_import") return;
    previewResponses.push(
      (await response.json()) as { error: string | null; revision?: number },
    );
  });
  const before = await readWorkspaceDocument(page);
  await page.getByTestId("production-context-front-silkscreen").click();
  await page.getByTestId("image-file-input").setInputFiles(
    path.resolve(process.cwd(), "../../assets/branding/pcb-atelier-logo.png"),
  );
  const importer = page.getByRole("dialog", { name: "图片导入处理器" });
  await expect(importer).toBeVisible();
  await expect(importer.getByText("原图", { exact: true })).toBeVisible();
  await expect(importer.getByText("处理结果", { exact: true })).toBeVisible();
  expect(await readWorkspaceDocument(page)).toEqual(before);
  const confirm = importer.getByRole("button", { name: "确认处理并插入" });
  await expect(confirm).toBeEnabled();
  previewRequests.length = 0;
  previewResponses.length = 0;
  const processedPreview = importer.getByRole("img", {
    name: "pcb-atelier-logo.png 处理结果",
  });
  const initialPreview = await processedPreview.getAttribute("style");
  await importer
    .getByRole("slider", { name: "平滑半径 mm 快速调节" })
    .fill("0.5");
  await expect(confirm).toBeDisabled();
  await expect(importer.getByLabel("实时预览状态")).toContainText(
    "正在实时更新预览",
  );
  await expect.poll(() => previewRequests.length).toBe(1);
  await expect.poll(() => previewResponses.length).toBe(1);
  expect(previewResponses[0]?.error).toBeNull();
  await expect(confirm).toBeEnabled();
  await expect(importer.getByLabel("实时预览状态")).toContainText(
    "实时预览已更新",
  );
  await expect.poll(() => processedPreview.getAttribute("style")).not.toBe(
    initialPreview,
  );
  await confirm.click();
  await expect(page.getByRole("status")).toContainText("图片已处理并插入");

  const tree = page.getByRole("tree", { name: "板体与生产层" });
  await expect(
    tree
      .getByRole("group", { name: "正面" })
      .getByText("pcb-atelier-logo.png", { exact: true }),
  ).toBeVisible();
  const treatmentEditor = page.getByRole("region", { name: "图片处理" });
  await expect(treatmentEditor).toBeVisible();
  await expect(treatmentEditor.getByText("原图", { exact: true })).toBeVisible();
  await expect(
    treatmentEditor.getByText("处理结果", { exact: true }),
  ).toBeVisible();
  await expect(
    treatmentEditor.getByRole("button", { name: "临时查看原图" }),
  ).toBeVisible();
  expect(
    await page
      .getByTestId("workspace-inspector")
      .evaluate((element) => element.scrollWidth <= element.clientWidth),
  ).toBe(true);

  await page
    .getByRole("group", { name: "工作模式" })
    .getByRole("button", { name: "预览" })
    .click();
  await expect(page.getByTestId("board-3d-preview")).toBeVisible({
    timeout: 15_000,
  });
  await expect(page.getByTestId("board-3d-preview").locator("canvas")).toBeVisible();
});

test("取消图片导入不会留下素材、处理、图层或映射", async ({ page }) => {
  const before = await readWorkspaceDocument(page);
  await page.getByTestId("image-file-input").setInputFiles(
    path.resolve(process.cwd(), "../../assets/branding/pcb-atelier-logo.png"),
  );
  const importer = page.getByRole("dialog", { name: "图片导入处理器" });
  await expect(importer.getByRole("button", { name: "确认处理并插入" }))
    .toBeEnabled();
  await importer.getByRole("button", { name: "取消" }).click();
  await expect(page.getByRole("status")).toContainText("已取消图片导入");
  expect(await readWorkspaceDocument(page)).toEqual(before);
});

test("外部图片拖入画布时以实际落点作为对象中心", async ({
  page,
}) => {
  const imageInsertRequests: Array<{
    placementCenterUm?: { xUm: number; yUm: number };
  }> = [];
  page.on("request", (request) => {
    if (!request.url().endsWith("/__atelier_bridge")) return;
    const payload = request.postDataJSON() as {
      command?: string;
      args?: {
        request?: { placementCenterUm?: { xUm: number; yUm: number } };
      };
    } | null;
    if (payload?.command === "confirm_image_import" && payload.args?.request) {
      imageInsertRequests.push(payload.args.request);
    }
  });
  const canvas = page.getByTestId("workspace-canvas-front");
  const canvasBox = await canvas.boundingBox();
  if (!canvasBox) throw new Error("正面画板不可见");
  const board = (await readWorkspaceDocument(page)).board;
  const externalPoint = { xUm: 18_000, yUm: 35_000 };
  const beforeExternal = await readWorkspaceDocument(page);
  await dropPngFileAtBoardPoint(
    canvas,
    "drop-external.png",
    board,
    externalPoint,
  );
  const importer = page.getByRole("dialog", { name: "图片导入处理器" });
  await expect(importer.getByRole("button", { name: "确认处理并插入" }))
    .toBeEnabled();
  expect(await readWorkspaceDocument(page)).toEqual(beforeExternal);
  await importer.getByRole("button", { name: "确认处理并插入" }).click();
  await expect(page.getByRole("status")).toContainText(
    "图片已处理并放置到拖放位置",
  );
  const externalPlacement = imageInsertRequests.at(-1)?.placementCenterUm;
  expect(externalPlacement).toBeTruthy();
  expect(externalPlacement).not.toEqual({
    xUm: board.widthUm / 2,
    yUm: board.heightUm / 2,
  });

  const afterExternal = await readWorkspaceDocument(page);
  const priorExternalIds = new Set(
    beforeExternal.frontLayers.map((layer) => layer.id),
  );
  const external = afterExternal.frontLayers.find(
    (layer) =>
      layer.name === "drop-external.png" && !priorExternalIds.has(layer.id),
  );
  expect(external).toBeTruthy();
  expect(layerCenter(external!.transform)).toEqual(externalPlacement);
});

test("Web 左树多选可以通过共享服务完成分组与解组", async ({ page }) => {
  await page.getByTestId("production-context-back-silkscreen").click();
  const tree = page.getByRole("tree", { name: "板体与生产层" });
  const back = tree.getByRole("group", { name: "背面" });
  const mark = back.getByRole("treeitem").filter({ hasText: "背面标记" });
  const note = back.getByRole("treeitem").filter({ hasText: "背面说明" });
  await expect(
    page.getByRole("button", { name: "分组", exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "解组", exact: true }),
  ).toHaveCount(0);

  await mark.getByRole("button", { name: "背面标记", exact: true }).click();
  await note
    .getByRole("button", { name: "背面说明", exact: true })
    .click({ modifiers: ["Shift"] });

  const groupButton = page.getByRole("button", { name: "分组", exact: true });
  await expect(groupButton).toBeEnabled();
  await groupButton.click();
  await expect(page.getByRole("status")).toContainText("已分组");
  await expect(groupButton).toHaveCount(0);
  const group = back.getByRole("treeitem").filter({ hasText: "组合" });
  await expect(group.getByText("组合", { exact: true })).toBeVisible();

  await mark.getByRole("button", { name: "背面标记", exact: true }).click();
  await note
    .getByRole("button", { name: "背面说明", exact: true })
    .click({ modifiers: ["Meta"] });
  await expect(mark).toHaveAttribute("aria-selected", "true");
  await expect(note).toHaveAttribute("aria-selected", "true");
  await group.getByRole("button", { name: "组合", exact: true }).click();

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

test("左树拖拽支持进入组合、组内排序与拖出组合", async ({ page }) => {
  await page.getByTestId("production-context-back-silkscreen").click();
  const tree = page.getByRole("tree", { name: "板体与生产层" });
  const back = tree.getByRole("group", { name: "背面" });
  const mark = back.getByRole("treeitem").filter({ hasText: "背面标记" });
  const note = back.getByRole("treeitem").filter({ hasText: "背面说明" });

  await mark.getByRole("button", { name: "背面标记", exact: true }).click();
  await note
    .getByRole("button", { name: "背面说明", exact: true })
    .click({ modifiers: ["Shift"] });
  await page.getByRole("button", { name: "分组", exact: true }).click();
  const group = back.getByRole("treeitem").filter({ hasText: "组合" });

  await page.getByRole("button", { name: "文字工具 (T)" }).click();
  await clickCanvas(page.getByTestId("workspace-canvas-back"));
  const editor = page.getByTestId("text-editor");
  await editor.fill("待拖拽");
  await editor.press("Escape");
  await page.getByRole("button", { name: "选择工具 (V)" }).click();

  const added = back
    .getByRole("treeitem")
    .filter({ hasText: "文字" })
    .last();
  await expect(added).toHaveCSS("padding-left", "2px");

  await pointerDrag(page, added, group);
  await expect(page.getByRole("status")).toContainText("图层层级与顺序已更新");
  await expect(added).toHaveCSS("padding-left", "16px");

  await pointerDrag(page, added, mark, { xRatio: 0.1, yRatio: 0.05 });
  await expect
    .poll(async () => {
      const labels = await back.getByRole("treeitem").allTextContents();
      return labels.findIndex((label) => label.includes("文字")) <
        labels.findIndex((label) => label.includes("背面标记"));
    })
    .toBe(true);

  await pointerDrag(page, added, group, { xRatio: 0.1, yRatio: 0.95 });
  await expect(added).toHaveCSS("padding-left", "2px");
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
  await expect(page.getByRole("button", { name: "撤销" })).toHaveCount(0);
  await page.getByRole("button", { name: "工程菜单" }).click();
  await page.getByRole("menuitem", { name: "撤销" }).click();
  await expect(tree.getByText("基础铺铜", { exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: "工程菜单" }).click();
  await page.getByRole("menuitem", { name: "重做" }).click();
  await expect(tree.getByText("基础铺铜", { exact: true })).toHaveCount(1);
});

test("对象可在物理生产层之间拖动且日常界面不暴露关联概念", async ({ page }) => {
  const tree = page.getByRole("tree", { name: "板体与生产层" });
  await page.getByTestId("production-context-back-silkscreen").click();
  await page.getByTestId("production-context-back-copper").click();
  const silk = page.getByTestId("production-layer-back-silkscreen");
  const copper = page.getByTestId("production-layer-back-copper");
  const note = silk.getByRole("treeitem").filter({ hasText: "背面说明" });

  await note.scrollIntoViewIfNeeded();
  const sourceBox = await note
    .getByRole("button", { name: "背面说明", exact: true })
    .boundingBox();
  const targetBox = await page
    .getByTestId("production-context-back-copper")
    .boundingBox();
  expect(sourceBox).not.toBeNull();
  expect(targetBox).not.toBeNull();
  const sourceX = sourceBox!.x + sourceBox!.width / 2;
  const sourceY = sourceBox!.y + sourceBox!.height / 2;
  await page.mouse.move(sourceX, sourceY);
  await page.mouse.down();
  await page.mouse.move(sourceX + 6, sourceY + 6, { steps: 2 });
  await page.mouse.move(
    targetBox!.x + targetBox!.width / 2,
    targetBox!.y + targetBox!.height / 2,
    { steps: 8 },
  );
  await expect(page.locator("html")).toHaveCSS("cursor", "grabbing");
  await page.mouse.up();
  await expect(page.getByRole("status")).toContainText(
    "对象已移动到新的生产层",
  );
  await expect(note).toHaveCount(0);
  await expect(
    copper
      .getByRole("treeitem")
      .filter({ hasText: "背面说明" }),
  ).toBeVisible();
  await expect(page.getByText("关联到生产层")).toHaveCount(0);
});

test("复制与剪切可通过快捷键跨正反面粘贴", async ({ page }) => {
  const tree = page.getByRole("tree", { name: "板体与生产层" });
  const before = await readWorkspaceDocument(page);
  const sourceCopy = before.frontLayers.find((layer) => layer.name === "正面说明");
  const sourceCut = before.frontLayers.find((layer) => layer.name === "正面标题");
  if (!sourceCopy || !sourceCut) throw new Error("缺少跨面剪贴板测试对象");

  await tree
    .getByRole("treeitem")
    .filter({ hasText: "正面说明" })
    .getByRole("button", { name: "正面说明", exact: true })
    .click();
  await page.keyboard.press("Meta+c");
  await expect(page.getByRole("status")).toContainText("对象已复制");
  await clickCanvas(page.getByTestId("workspace-canvas-back"));
  await page.keyboard.press("Meta+v");
  await expect(page.getByRole("status")).toContainText("已粘贴到背面");

  await expect
    .poll(async () => {
      const document = await readWorkspaceDocument(page);
      return document.backLayers.find((layer) => layer.name === "正面说明 副本");
    })
    .toMatchObject({ transform: sourceCopy.transform });

  await tree
    .getByRole("treeitem")
    .filter({ hasText: "正面标题" })
    .getByRole("button", { name: "正面标题", exact: true })
    .click();
  await page.keyboard.press("Meta+x");
  await expect(page.getByRole("status")).toContainText("对象已剪切");
  await expect
    .poll(async () =>
      (await readWorkspaceDocument(page)).frontLayers.some(
        (layer) => layer.id === sourceCut.id,
      ),
    )
    .toBe(false);
  await clickCanvas(page.getByTestId("workspace-canvas-back"));
  await page.keyboard.press("Meta+v");
  await expect(page.getByRole("status")).toContainText("已移动到背面");

  await expect
    .poll(async () => {
      const document = await readWorkspaceDocument(page);
      return {
        frontHasSource: document.frontLayers.some(
          (layer) => layer.id === sourceCut.id,
        ),
        backHasSource: document.backLayers.some(
          (layer) => layer.id === sourceCut.id,
        ),
      };
    })
    .toEqual({ frontHasSource: false, backHasSource: true });
});

test("对象可从左侧树跨正反面拖到目标生产层", async ({ page }) => {
  const tree = page.getByRole("tree", { name: "板体与生产层" });
  const source = tree
    .getByRole("group", { name: "正面" })
    .getByRole("treeitem")
    .filter({ hasText: "正面说明" });
  const target = page.getByTestId("production-context-back-copper");

  await pointerDrag(page, source, target);

  await expect(page.getByRole("status")).toContainText("对象已移动到背面");
  await expect(source).toHaveCount(0);
  await expect
    .poll(async () =>
      (await readWorkspaceDocument(page)).backLayers.some(
        (layer) => layer.name === "正面说明",
      ),
    )
    .toBe(true);
});

test("非法跨层拖拽在松手前显示禁止反馈且不改变归属", async ({ page }) => {
  await page.getByTestId("production-context-front-copper").click();
  await page.getByRole("button", { name: "添加基础铺铜" }).click();
  const copper = page.getByTestId("production-layer-front-copper");
  const silk = page.getByTestId("production-layer-front-silkscreen");
  const fill = copper.getByRole("treeitem").filter({ hasText: "基础铺铜" });
  const target = page.getByTestId("production-context-front-silkscreen");
  const sourceBox = await fill.boundingBox();
  const targetBox = await target.boundingBox();
  expect(sourceBox).not.toBeNull();
  expect(targetBox).not.toBeNull();

  const startX = sourceBox!.x + sourceBox!.width / 2;
  const startY = sourceBox!.y + sourceBox!.height / 2;
  await page.mouse.move(startX, startY);
  await page.mouse.down();
  await page.mouse.move(startX + 6, startY + 6, { steps: 2 });
  await page.mouse.move(
    targetBox!.x + targetBox!.width / 2,
    targetBox!.y + targetBox!.height / 2,
    { steps: 8 },
  );
  await expect(page.locator("html")).toHaveCSS("cursor", "not-allowed");
  await page.mouse.up();

  await expect(fill).toBeVisible();
  await expect(
    silk.getByRole("treeitem").filter({ hasText: "基础铺铜" }),
  ).toHaveCount(0);
  await expect(page.getByText("移至当前生产层顶层")).toHaveCount(0);
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

async function readWorkspaceDocument(page: Page) {
  return page.evaluate(async () => {
    const response = await fetch("/__atelier_bridge", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        contractVersion: "pcb-atelier-workspace-v1",
        command: "get_workspace_document",
        args: {},
      }),
    });
    const result = (await response.json()) as {
      payload: {
        board: { widthUm: number; heightUm: number };
        frontLayers: Array<{
          id: string;
          name: string;
          transform: {
            xUm: number;
            yUm: number;
            widthUm: number;
            heightUm: number;
            rotationMdeg: number;
            flipX: boolean;
            flipY: boolean;
          };
        }>;
        backLayers: Array<{
          id: string;
          name: string;
          transform: {
            xUm: number;
            yUm: number;
            widthUm: number;
            heightUm: number;
            rotationMdeg: number;
            flipX: boolean;
            flipY: boolean;
          };
        }>;
        assets: unknown[];
        imageTreatments: unknown[];
        mappings: unknown[];
      };
    };
    return result.payload;
  });
}

async function dropPngFileAtBoardPoint(
  target: Locator,
  filename: string,
  board: { widthUm: number; heightUm: number },
  point: { xUm: number; yUm: number },
) {
  await target.evaluate(
    (element, request) => {
      const scale = 5.5;
      const bounds = element.getBoundingClientRect();
      const originX =
        bounds.width / 2 - (request.board.widthUm / 1_000) * scale / 2;
      const originY =
        bounds.height / 2 - (request.board.heightUm / 1_000) * scale / 2;
      const binary = atob(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==",
      );
      const bytes = Uint8Array.from(binary, (character) =>
        character.charCodeAt(0),
      );
      const dataTransfer = new DataTransfer();
      dataTransfer.items.add(
        new File([bytes], request.filename, { type: "image/png" }),
      );
      for (const type of ["dragover", "drop"]) {
        element.dispatchEvent(
          new DragEvent(type, {
            bubbles: true,
            cancelable: true,
            clientX:
              bounds.left + originX + request.point.xUm / 1_000 * scale,
            clientY:
              bounds.top + originY + request.point.yUm / 1_000 * scale,
            dataTransfer,
          }),
        );
      }
    },
    { board, filename, point },
  );
}

function layerCenter(transform: {
  xUm: number;
  yUm: number;
  widthUm: number;
  heightUm: number;
}) {
  return {
    xUm: transform.xUm + Math.floor(transform.widthUm / 2),
    yUm: transform.yUm + Math.floor(transform.heightUm / 2),
  };
}

async function pointerDrag(
  page: Page,
  source: Locator,
  target: Locator,
  position: { xRatio: number; yRatio: number } = {
    xRatio: 0.5,
    yRatio: 0.5,
  },
) {
  const sourceBox = await source.boundingBox();
  const targetBox = await target.boundingBox();
  expect(sourceBox).not.toBeNull();
  expect(targetBox).not.toBeNull();
  const startX = sourceBox!.x + sourceBox!.width / 2;
  const startY = sourceBox!.y + sourceBox!.height / 2;
  const targetX = targetBox!.x + targetBox!.width * position.xRatio;
  const targetY = targetBox!.y + targetBox!.height * position.yRatio;
  await page.mouse.move(startX, startY);
  await page.mouse.down();
  await page.mouse.move(startX + 6, startY + 6, { steps: 2 });
  await page.mouse.move(targetX, targetY, { steps: 8 });
  await page.mouse.up();
}
