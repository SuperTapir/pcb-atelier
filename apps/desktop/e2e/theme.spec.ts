import { expect, test } from "@playwright/test";

test("主题可切换、持久化，并在跟随系统时响应外观变化", async ({
  page,
}) => {
  await page.emulateMedia({ colorScheme: "light" });
  await page.goto("/");

  const theme = page.getByRole("combobox", { name: "界面主题" });
  const lightBackground = await page.evaluate(
    () => getComputedStyle(document.body).backgroundColor,
  );
  await theme.selectOption("dark");
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
  await expect(theme).toHaveValue("dark");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");

  await theme.selectOption("system");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
});
