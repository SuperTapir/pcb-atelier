import { expect, test, type Locator, type Page } from "./test";

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await expect(
    page.locator("header").getByText("双面非对称黄金卡", { exact: true }),
  ).toBeVisible();
  await page.getByTestId("board-root").click();
});

test("板面工艺只暴露日常外观项，并在入口阻止 FR-4 的 OSP", async ({
  page,
}) => {
  const finish = page.getByRole("combobox", { name: "露铜表面处理" });
  await expect(finish.locator('option[value="osp"]')).toHaveAttribute(
    "disabled",
    "",
  );
  await expect(
    page.getByRole("combobox", { name: "阻焊油墨" }),
  ).toBeVisible();
  await expect(
    page.getByRole("combobox", { name: "外层铜厚" }),
  ).toHaveCount(0);
  await expect(page.getByRole("combobox", { name: "铜层数" })).toHaveCount(0);
  await expect(page.getByRole("combobox", { name: "字符工艺" })).toHaveCount(
    0,
  );
});

test("编辑态阻焊即时同步换色且保持几何、选择和正背视口", async ({
  page,
}) => {
  const tree = page.getByRole("tree", { name: "板体与生产层" });
  const noteButton = tree.getByRole("button", {
    name: "正面说明",
    exact: true,
  });
  const note = noteButton.locator("..");
  await noteButton.click();
  const front = page.getByTestId("workspace-canvas-front");
  const back = page.getByTestId("workspace-canvas-back");
  const viewportBefore = await Promise.all([
    viewportAttributes(front),
    viewportAttributes(back),
  ]);
  const documentBefore = await readWorkspaceDocument(page);

  await page.getByTestId("board-root").click();
  await page.getByRole("combobox", { name: "阻焊油墨" }).selectOption("purple");

  await expect(page.getByRole("status")).toContainText(
    "制造参数与工艺近似已更新",
  );
  await expect(front).toHaveAttribute("data-solder-mask-color", "#5b2f70");
  await expect(back).toHaveAttribute("data-solder-mask-color", "#5b2f70");
  await expect(note).toHaveAttribute("aria-selected", "true");
  expect(await viewportAttributes(front)).toEqual(viewportBefore[0]);
  expect(await viewportAttributes(back)).toEqual(viewportBefore[1]);
  const documentAfter = await readWorkspaceDocument(page);
  expect(documentAfter.frontLayers).toEqual(documentBefore.frontLayers);
  expect(documentAfter.backLayers).toEqual(documentBefore.backLayers);
  expect(documentAfter.mappings).toEqual(documentBefore.mappings);
});

test("表面处理只更新材质语义，不改变生产几何签名", async ({ page }) => {
  await page
    .getByRole("combobox", { name: "露铜表面处理" })
    .selectOption("enig");
  const inspector = page.getByRole("region", { name: "板面工艺" });
  const before = await inspector.getAttribute("data-geometry-signature");
  const finish = page.getByRole("combobox", { name: "露铜表面处理" });
  const front = page.getByTestId("workspace-canvas-front");
  const back = page.getByTestId("workspace-canvas-back");
  const documentBefore = await readWorkspaceDocument(page);
  await expect(front).toHaveAttribute("data-exposed-copper-color", "#d3a639");
  await expect(back).toHaveAttribute("data-exposed-copper-color", "#d3a639");

  await finish.selectOption("haslLeadFree");

  await expect(page.getByRole("status")).toContainText(
    "制造参数与工艺近似已更新",
  );
  await expect(inspector).toHaveAttribute("data-geometry-signature", before!);
  await expect(front).toHaveAttribute("data-exposed-copper-color", "#c3c7c9");
  await expect(back).toHaveAttribute("data-exposed-copper-color", "#c3c7c9");
  await expect(page.getByTestId("material-semantics")).toContainText(
    "有铜开窗 · 表面处理",
  );
  const documentAfter = await readWorkspaceDocument(page);
  expect(documentAfter.frontLayers).toEqual(documentBefore.frontLayers);
  expect(documentAfter.backLayers).toEqual(documentBefore.backLayers);
  expect(documentAfter.mappings).toEqual(documentBefore.mappings);
});

async function viewportAttributes(canvas: Locator) {
  return canvas.evaluate((element) => ({
    panX: (element as HTMLElement).dataset.viewportPanX,
    panY: (element as HTMLElement).dataset.viewportPanY,
    zoom: (element as HTMLElement).dataset.viewportZoom,
  }));
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
        frontLayers: unknown[];
        backLayers: unknown[];
        mappings: unknown[];
      };
    };
    return result.payload;
  });
}
