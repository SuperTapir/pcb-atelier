import { expect, test, type Page } from "./test";

const CONTRACT_VERSION = "pcb-atelier-workspace-v1";

test("项目媒体可新建路径、移动素材并保护非法路径与去重状态", async ({
  page,
}) => {
  await page.goto("/");
  const bytes = [137, 80, 78, 71, 13, 10, 26, 10, 31, 41, 59, 26];
  const imported = await invokeBridge(page, "import_project_asset", {
    request: {
      originalFilename: "folder-e2e-logo.png",
      mediaType: "image/png",
      pixelWidth: 2,
      pixelHeight: 2,
      bytes,
    },
  });
  expect(imported.error).toBeNull();
  const assetId = imported.payload.assetId as string;
  const assetCount = (imported.payload.document as WorkspaceDocument).assets.length;

  await page.reload();
  const media = page.getByRole("region", { name: "项目媒体" });
  await media
    .getByRole("button", {
      name: "folder-e2e-logo.png，点击预览，可拖到正面丝印层",
    })
    .click();
  await page.getByRole("button", { name: "关闭素材预览" }).click();

  const folderPath = page.getByRole("combobox", { name: "素材文件夹路径" });
  await folderPath.fill("../外部");
  await page.getByRole("button", { name: "移动素材" }).click();
  await expect(page.getByRole("alert")).toContainText("不能包含");

  const afterRejected = await invokeBridge(page, "get_workspace_document", {});
  const rejectedAsset = (afterRejected.payload as WorkspaceDocument).assets.find(
    (asset) => asset.id === assetId,
  );
  expect(rejectedAsset?.folderPath).toBeNull();

  await folderPath.fill(" 活动 / 图标 ");
  await page.getByRole("button", { name: "移动素材" }).click();
  await expect(page.getByRole("status")).toContainText("素材已移至 活动/图标");
  await expect(media.getByText("活动/图标", { exact: true })).toBeVisible();

  const search = page.getByRole("searchbox", { name: "搜索项目媒体" });
  await search.fill("活动");
  await expect(
    media.getByRole("button", {
      name: "folder-e2e-logo.png，点击预览，可拖到正面丝印层",
    }),
  ).toContainText("未使用");

  const repeated = await invokeBridge(page, "import_project_asset", {
    request: {
      originalFilename: "same-content-new-name.png",
      mediaType: "image/png",
      pixelWidth: 2,
      pixelHeight: 2,
      bytes,
    },
  });
  expect(repeated.error).toBeNull();
  expect(repeated.payload.reused).toBe(true);
  expect(repeated.payload.assetId).toBe(assetId);
  expect((repeated.payload.document as WorkspaceDocument).assets).toHaveLength(
    assetCount,
  );
  expect(
    (repeated.payload.document as WorkspaceDocument).assets.find(
      (asset) => asset.id === assetId,
    )?.folderPath,
  ).toBe("活动/图标");
});

async function invokeBridge(
  page: Page,
  command: string,
  args: Record<string, unknown>,
) {
  return page.evaluate(
    async ({ contractVersion, command, args }) => {
      const response = await fetch("/__atelier_bridge", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ contractVersion, command, args }),
      });
      return (await response.json()) as {
        error: string | null;
        payload: Record<string, unknown>;
      };
    },
    { contractVersion: CONTRACT_VERSION, command, args },
  );
}

interface WorkspaceDocument {
  assets: Array<{
    id: string;
    folderPath: string | null;
  }>;
}
