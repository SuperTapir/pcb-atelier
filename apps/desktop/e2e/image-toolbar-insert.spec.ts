import path from "node:path";

import { expect, test, type Page } from "./test";

test("点击图片工具可选择文件并完成图片插入", async ({ page }) => {
  await page.goto("/");
  await expect(
    page.locator("header").getByText("双面非对称黄金卡", { exact: true }),
  ).toBeVisible();
  await page.getByTestId("production-context-front-silkscreen").click();

  const fileChooserPromise = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: "图片工具 (P)" }).click();
  const fileChooser = await fileChooserPromise;
  await fileChooser.setFiles(
    path.resolve(process.cwd(), "../../assets/branding/pcb-atelier-logo.png"),
  );

  const importer = page.getByRole("dialog", { name: "图片导入处理器" });
  await expect(importer).toBeVisible();
  const dialogSurface = importer.locator(
    '[data-scroll-policy="dialog-no-scroll"]',
  );
  await expect
    .poll(() =>
      dialogSurface.evaluate(
        (element) => element.scrollHeight <= element.clientHeight,
      ),
    )
    .toBe(true);
  const previewImages = importer.locator(
    '[data-preview-size="compact"] [role="img"][data-fit-mode="contain"]',
  );
  await expect(previewImages).toHaveCount(2);
  for (const previewImage of await previewImages.all()) {
    await expect
      .poll(() =>
        previewImage.evaluate(
          (image) =>
            getComputedStyle(image).backgroundSize === "contain" &&
            getComputedStyle(image).backgroundRepeat === "no-repeat" &&
            getComputedStyle(image).backgroundPosition === "50% 50%",
        ),
      )
      .toBe(true);
  }
  await expect(
    importer.getByRole("radio", { name: "彩色原图" }),
  ).toBeEnabled();
  await expect(
    importer.getByRole("radio", { name: "彩色原图" }),
  ).toHaveAttribute("title", /彩色丝印/);
  await expect(
    importer.getByRole("button", { name: "确认处理并插入" }),
  ).toBeEnabled();
  await importer.getByRole("button", { name: "开始裁切" }).click();
  const cropDialog = page.getByRole("dialog", { name: "裁切图片" });
  await expect(cropDialog).toBeVisible();
  await expect(
    cropDialog.getByRole("button", { name: "移动裁切区域" }),
  ).toBeVisible();
  const resizeHandle = cropDialog.getByRole("button", {
    name: "从右下角调整裁切区域",
  });
  await resizeHandle.click();
  const resizeBounds = await resizeHandle.boundingBox();
  if (!resizeBounds) throw new Error("裁切手柄不可见");
  await page.mouse.move(
    resizeBounds.x + resizeBounds.width / 2,
    resizeBounds.y + resizeBounds.height / 2,
  );
  await page.mouse.down();
  await page.mouse.move(resizeBounds.x - 30, resizeBounds.y - 30);
  await page.mouse.up();
  await cropDialog.getByText("精确数值", { exact: true }).click();
  await expect
    .poll(async () =>
      Number(
        await cropDialog
          .getByRole("spinbutton", { name: "裁切宽度 %" })
          .inputValue(),
      ),
    )
    .toBeLessThan(100);
  await cropDialog.getByRole("button", { name: "完成裁切" }).click();
  await expect(cropDialog).toHaveCount(0);
  await expect(importer.getByText("当前保留")).toBeVisible();
  await expect(
    importer.getByRole("button", { name: "确认处理并插入" }),
  ).toBeEnabled();
  await importer.getByRole("button", { name: "确认处理并插入" }).click();

  await expect(
    page.getByText("图片已处理并插入", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "选择工具 (V)" }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(
    page.getByRole("button", { name: "图片工具 (P)" }),
  ).toHaveAttribute("aria-pressed", "false");
  await expect(
    page
      .getByRole("tree", { name: "板体与生产层" })
      .getByRole("group", { name: "正面" })
      .getByText("pcb-atelier-logo.png", { exact: true }),
  ).toBeVisible();
});

test("满足制造约束时可按彩色原图导入并保持彩色生产预览", async ({
  page,
}) => {
  await page.goto("/");
  await expect(
    page.locator("header").getByText("双面非对称黄金卡", { exact: true }),
  ).toBeVisible();
  await page.getByTestId("production-context-front-silkscreen").click();

  const input = page.getByTestId("image-file-input");
  await input.setInputFiles(
    path.resolve(process.cwd(), "../../assets/branding/pcb-atelier-logo.png"),
  );

  const importer = page.getByRole("dialog", { name: "图片导入处理器" });
  await expect(importer).toBeVisible();
  const colorOriginal = importer.getByRole("radio", { name: "彩色原图" });
  await expect(colorOriginal).toBeEnabled();
  await colorOriginal.click();
  await expect(colorOriginal).toHaveAttribute("aria-checked", "true");
  const colorPreview = importer.locator(
    '[data-preview-kind="color-original"]',
  );
  await expect(colorPreview).toBeVisible();
  await expect(
    colorPreview.getByRole("img", {
      name: "pcb-atelier-logo.png 彩色生产预览",
    }),
  ).toBeVisible();
  await expect(importer).toContainText("彩色原图将作为丝印生产资料");

  await importer.getByRole("button", { name: "确认处理并插入" }).click();
  await expect(importer).toHaveCount(0);
  await expect(
    page.getByText("图片已处理并插入", { exact: true }),
  ).toBeVisible();

  const document = await readWorkspaceDocument(page);
  expect(document.imageTreatments).toHaveLength(1);
  const treatment = document.imageTreatments[0];
  expect(treatment?.productionMode).toBe("colorOriginal");
  const treatmentMappings = document.mappings.filter(
    (mapping) => mapping.treatmentId === treatment?.id,
  );
  expect(treatmentMappings).toHaveLength(1);
  expect(treatmentMappings[0]?.target.layer).toBe("silkscreen");

  const editor = page.getByTestId("workspace-inspector");
  await expect(
    editor.getByRole("radio", { name: "彩色原图" }),
  ).toHaveAttribute("aria-checked", "true");
  await expect(
    editor.locator('[data-preview-kind="color-original"]'),
  ).toBeVisible();
  await expect(page.getByTestId("workspace-canvas-front")).toBeVisible();
});

test("图片入口拒绝非图片与伪造图片内容", async ({ page }) => {
  await page.goto("/");
  const input = page.getByTestId("image-file-input");
  await expect(input).toHaveAttribute(
    "accept",
    ".png,.jpg,.jpeg,.webp,image/png,image/jpeg,image/webp",
  );

  await input.setInputFiles({
    name: "notes.txt",
    mimeType: "text/plain",
    buffer: Buffer.from("plain text"),
  });
  await expect(page.getByRole("status")).toContainText(
    "仅支持 PNG、JPEG 或 WebP 图片",
  );
  await expect(
    page.getByRole("dialog", { name: "图片导入处理器" }),
  ).toHaveCount(0);

  await input.setInputFiles({
    name: "fake.png",
    mimeType: "image/png",
    buffer: Buffer.from("not really a png"),
  });
  await expect(page.getByRole("status")).toContainText(
    "文件内容不是有效的 PNG、JPEG 或 WebP 图片",
  );
  await expect(
    page.getByRole("dialog", { name: "图片导入处理器" }),
  ).toHaveCount(0);
});

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
        imageTreatments: Array<{
          id: string;
          productionMode: "monochromeMask" | "colorOriginal";
        }>;
        mappings: Array<{
          treatmentId: string | null;
          target: {
            layer: "copper" | "solderMaskOpen" | "silkscreen";
          };
        }>;
      };
    };
    return result.payload;
  });
}
