import { expect, test, type Locator, type Page } from "./test";

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await expect(
    page.locator("header").getByText("双面非对称黄金卡", { exact: true }),
  ).toBeVisible();
});

test("V/T/P、创建副本与删除共享统一快捷键作用域", async ({ page }) => {
  const selectTool = page.getByRole("button", { name: "选择工具 (V)" });
  const textTool = page.getByRole("button", { name: "文字工具 (T)" });
  const imageTool = page.getByRole("button", { name: "图片工具 (P)" });
  await page.keyboard.press("t");
  await expect(textTool).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press("v");
  await expect(selectTool).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press("p");
  await expect(imageTool).toHaveAttribute("aria-pressed", "true");

  const tree = page.getByRole("tree", { name: "板体与生产层" });
  const source = tree.getByRole("button", {
    name: "正面说明",
    exact: true,
  });
  await source.click();
  await page.keyboard.press("Meta+d");
  const duplicate = tree.getByRole("button", {
    name: "正面说明 副本",
    exact: true,
  });
  await expect(duplicate).toBeVisible();

  await page.keyboard.press("Delete");
  await expect(duplicate).toHaveCount(0);
  await page.keyboard.press("Meta+z");
  await expect(duplicate).toBeVisible();
  await page.keyboard.press("Meta+Shift+z");
  await expect(page.getByRole("status")).toContainText("已重做");
  await expect(duplicate).toHaveCount(0);
});

test("分组/解组快捷键与紧凑上下文操作条产生相同状态", async ({ page }) => {
  await page.getByTestId("production-context-back-silkscreen").click();
  const back = page
    .getByRole("tree", { name: "板体与生产层" })
    .getByRole("group", { name: "背面" });
  const mark = back.getByRole("button", { name: "背面标记", exact: true });
  const note = back.getByRole("button", { name: "背面说明", exact: true });

  await expect(
    page.getByRole("button", { name: "分组", exact: true }),
  ).toHaveCount(0);
  await mark.click();
  await note.click({ modifiers: ["Shift"] });
  await expect(
    page.getByRole("button", { name: "分组", exact: true }),
  ).toBeVisible();

  await page.keyboard.press("Meta+g");
  const group = back.getByRole("button", { name: "组合", exact: true });
  await expect(group).toBeVisible();
  await expect(
    page.getByRole("button", { name: "解组", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "分组", exact: true }),
  ).toHaveCount(0);

  await page.keyboard.press("Meta+Shift+g");
  await expect(group).toHaveCount(0);
  await expect(mark).toBeVisible();
  await expect(note).toBeVisible();
});

test("右键菜单与紧凑操作条复用解组、分组和副本命令", async ({ page }) => {
  await page.getByTestId("production-context-back-silkscreen").click();
  const back = page
    .getByRole("tree", { name: "板体与生产层" })
    .getByRole("group", { name: "背面" });
  const mark = back.getByRole("button", { name: "背面标记", exact: true });
  const note = back.getByRole("button", { name: "背面说明", exact: true });

  await mark.click();
  await note.click({ modifiers: ["Shift"] });
  await expect(mark.locator("..")).toHaveAttribute("aria-selected", "true");
  await expect(note.locator("..")).toHaveAttribute("aria-selected", "true");
  await expect(
    page.getByRole("button", { name: "分组", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "分组", exact: true }).click();
  let group = back.getByRole("button", { name: "组合", exact: true });
  await expect(group).toBeVisible();
  await rightClickLayer(page, "back", "组合");
  const objectMenu = page.getByRole("menu", { name: "对象菜单" });
  await objectMenu.getByRole("menuitem", { name: "解组", exact: true }).click();
  await expect(group).toHaveCount(0);
  await expect(mark).toBeVisible();
  await expect(note).toBeVisible();

  await mark.click();
  await note.click({ modifiers: ["Shift"] });
  await page.getByRole("button", { name: "分组", exact: true }).click();
  group = back.getByRole("button", { name: "组合", exact: true });
  await expect(group).toBeVisible();
  await page.keyboard.press("Meta+Shift+g");
  await expect(group).toHaveCount(0);

  await note.click();
  await rightClickLayer(page, "back", "背面说明");
  await objectMenu
    .getByRole("menuitem", { name: "创建副本", exact: true })
    .click();
  const duplicate = back.getByRole("button", {
    name: "背面说明 副本",
    exact: true,
  });
  await expect(duplicate).toBeVisible();
  await page.keyboard.press("Delete");
  await expect(duplicate).toHaveCount(0);
});

test("左侧图层菜单提供复制、剪切、创建副本、显隐和删除", async ({ page }) => {
  await page.getByTestId("production-context-back-silkscreen").click();
  const back = page
    .getByRole("tree", { name: "板体与生产层" })
    .getByRole("group", { name: "背面" });
  const note = back.getByRole("button", { name: "背面说明", exact: true });

  await note.click({ button: "right" });
  let menu = page.getByRole("menu", { name: "背面说明 图层菜单" });
  for (const label of [
    "重命名",
    "复制",
    "剪切",
    "创建副本",
    "隐藏",
    "锁定",
    "删除",
  ]) {
    await expect(menu.getByRole("menuitem", { name: label, exact: true })).toBeVisible();
  }

  await menu
    .getByRole("menuitem", { name: "创建副本", exact: true })
    .click();
  const duplicate = back.getByRole("button", {
    name: "背面说明 副本",
    exact: true,
  });
  await expect(duplicate).toBeVisible();

  await duplicate.click({ button: "right" });
  menu = page.getByRole("menu", { name: "背面说明 副本 图层菜单" });
  await menu.getByRole("menuitem", { name: "删除", exact: true }).click();
  await expect(duplicate).toHaveCount(0);
});

test("画布文字编辑会隔离单键工具、删除和撤销快捷键", async ({ page }) => {
  const selectTool = page.getByRole("button", { name: "选择工具 (V)" });
  const source = page
    .getByRole("tree", { name: "板体与生产层" })
    .getByRole("button", { name: "背面说明", exact: true });
  await page.getByTestId("production-context-back-silkscreen").click();
  await source.click();
  await rightClickLayer(page, "back", "背面说明");
  await page
    .getByRole("menu", { name: "对象菜单" })
    .getByRole("menuitem", { name: "编辑文字", exact: true })
    .click();

  const editor = page.getByTestId("text-editor");
  await expect(editor).toBeFocused();
  await editor.fill("scope");
  await page.keyboard.type("vtp");
  await page.keyboard.press("Backspace");
  await expect(editor).toHaveValue("scopevt");
  await page.keyboard.press("Meta+z");
  await expect(editor).toBeFocused();
  await expect(selectTool).toHaveAttribute("aria-pressed", "true");
  await expect(source).toBeVisible();

  await page.keyboard.press("Meta+Enter");
  await expect(editor).toHaveCount(0);
  await expect(source).toBeVisible();
});

async function rightClickLayer(
  page: Page,
  face: "front" | "back",
  name: string,
) {
  const document = await readWorkspaceDocument(page);
  const layer = (
    face === "front" ? document.frontLayers : document.backLayers
  ).find((candidate) => candidate.name === name);
  if (!layer) throw new Error(`缺少画布对象：${name}`);
  const canvas = page.getByTestId(`workspace-canvas-${face}`);
  const bounds = await requiredBox(canvas);
  const viewport = await canvas.evaluate((element) => ({
    panX: Number((element as HTMLElement).dataset.viewportPanX),
    panY: Number((element as HTMLElement).dataset.viewportPanY),
    zoom: Number((element as HTMLElement).dataset.viewportZoom),
  }));
  const scale = 5.5 * viewport.zoom;
  const originX =
    bounds.x +
    bounds.width / 2 -
    ((document.board.widthUm / 1_000) * scale) / 2 +
    viewport.panX;
  const originY =
    bounds.y +
    bounds.height / 2 -
    ((document.board.heightUm / 1_000) * scale) / 2 +
    viewport.panY;
  await page.mouse.click(
    originX +
      ((layer.transform.xUm + layer.transform.widthUm / 2) / 1_000) * scale,
    originY +
      ((layer.transform.yUm + layer.transform.heightUm / 2) / 1_000) * scale,
    { button: "right" },
  );
}

async function requiredBox(locator: Locator) {
  const box = await locator.boundingBox();
  if (!box) throw new Error("目标不可见");
  return box;
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
        frontLayers: CanvasLayer[];
        backLayers: CanvasLayer[];
      };
    };
    return result.payload;
  });
}

interface CanvasLayer {
  name: string;
  transform: {
    xUm: number;
    yUm: number;
    widthUm: number;
    heightUm: number;
  };
}
