import { expect, test } from "./test";

test("3D 冷编译保留旧预览可交互并拒绝较旧结果回写", async ({ page }) => {
  let previewRequestCount = 0;
  let releaseSecondPreview!: () => void;
  const secondPreviewReleased = new Promise<void>((resolve) => {
    releaseSecondPreview = resolve;
  });

  await page.route("**/__atelier_bridge", async (route) => {
    const request = route.request().postDataJSON() as {
      command?: string;
    } | null;
    if (request?.command !== "get_board_preview") {
      await route.continue();
      return;
    }

    previewRequestCount += 1;
    const requestNumber = previewRequestCount;
    const response = await route.fetch();
    if (requestNumber === 2) await secondPreviewReleased;
    await route.fulfill({ response });
  });

  await page.goto("/");
  await expect(
    page.locator("header").getByText("双面非对称黄金卡", { exact: true }),
  ).toBeVisible();

  const modes = page.getByRole("group", { name: "工作模式" });
  await modes.getByRole("button", { name: "预览" }).click();
  const preview = page.getByTestId("board-3d-preview");
  await expect(preview).toBeVisible({ timeout: 15_000 });

  await modes.getByRole("button", { name: "编辑" }).click();
  await page.getByTestId("board-root").click();
  await page.getByRole("combobox", { name: "阻焊油墨" }).selectOption("black");
  await modes.getByRole("button", { name: "预览" }).click();

  await expect(preview).toBeVisible();
  await expect(
    page.getByRole("status", { name: "正在更新 3D 成板预览" }),
  ).toBeVisible();
  const canvas = preview.locator("canvas");
  const box = await canvas.boundingBox();
  expect(box).not.toBeNull();
  await page.mouse.move(box!.x + box!.width / 2, box!.y + box!.height / 2);
  await page.mouse.down();
  await page.mouse.move(box!.x + box!.width / 2 + 40, box!.y + box!.height / 2 + 20);
  await page.mouse.up();

  await modes.getByRole("button", { name: "编辑" }).click();
  await page.getByTestId("board-root").click();
  await page.getByRole("combobox", { name: "阻焊油墨" }).selectOption("white");
  await modes.getByRole("button", { name: "预览" }).click();
  await expect(preview).toHaveAttribute(
    "data-solder-mask-rgb",
    "226,228,222",
    { timeout: 15_000 },
  );

  releaseSecondPreview();
  await page.waitForTimeout(300);
  await expect(preview).toHaveAttribute(
    "data-solder-mask-rgb",
    "226,228,222",
  );
});
