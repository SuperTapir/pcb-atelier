import { expect, test } from "./test";

test("主题可切换、持久化，并在跟随系统时响应外观变化", async ({
  page,
}) => {
  await page.emulateMedia({ colorScheme: "light" });
  await page.goto("/");

  await page.getByRole("button", { name: "工程菜单" }).click();
  await page.getByRole("menuitem", { name: "设置…" }).click();
  let theme = page.getByRole("group", { name: "界面外观" });
  const lightBackground = await page.evaluate(
    () => getComputedStyle(document.body).backgroundColor,
  );
  await theme.getByRole("button", { name: "深色" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await expect
    .poll(() =>
      page.evaluate(() => document.documentElement.style.colorScheme),
    )
    .toBe("dark");
  expect(
    await page.evaluate(() => getComputedStyle(document.body).backgroundColor),
  ).not.toBe(lightBackground);

  await page.reload();
  await page.getByRole("button", { name: "工程菜单" }).click();
  await page.getByRole("menuitem", { name: "设置…" }).click();
  theme = page.getByRole("group", { name: "界面外观" });
  await expect(
    theme.getByRole("button", { name: "深色" }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");

  await theme.getByRole("button", { name: "跟随系统" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
});
