import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";

test("compact header keeps the brand and controls separate", async ({
  page,
}) => {
  await page.route("**/api/v1/me", (route) =>
    route.fulfill({ status: 401, json: { message: "Sign in first" } }),
  );
  await page.goto("/");
  await page.evaluate(() => document.fonts.ready);

  for (const width of [320, 375, 700]) {
    await page.setViewportSize({ width, height: 812 });
    const brand = await page.locator(".site-header .brand").boundingBox();
    const controls = await page.locator(".header-actions").boundingBox();
    expect(brand).not.toBeNull();
    expect(controls).not.toBeNull();
    expect(brand!.x + brand!.width).toBeLessThanOrEqual(controls!.x);
    expect(controls!.x + controls!.width).toBeLessThanOrEqual(width);
  }
});

async function headerGeometry(page: Page) {
  return page.locator(".header-inner").evaluate((header) => {
    const elements = [
      header,
      ...header.querySelectorAll(
        ".brand, .main-nav, .main-nav a, .theme-toggle",
      ),
    ];
    return elements.map((element) => {
      const { x, y, width, height } = element.getBoundingClientRect();
      return [x, y, width, height];
    });
  });
}

test("header stays fixed while changing pages and themes", async ({ page }) => {
  await page.route("**/api/v1/me", (route) =>
    route.fulfill({ status: 401, json: { message: "Sign in first" } }),
  );
  await page.emulateMedia({ colorScheme: "dark", reducedMotion: "reduce" });
  await page.goto("/");
  const navigation = page.locator("#main-navigation");
  await expect(
    navigation.getByRole("link", {
      name: "Home",
      exact: true,
      includeHidden: true,
    }),
  ).toHaveAttribute("aria-current", "page");
  await page.evaluate(() => document.fonts.ready);
  const original = await headerGeometry(page);

  for (const [label, path] of [
    ["Market", "/market"],
    ["My portfolio", "/portfolio"],
    ["Docs", "/docs"],
    ["Home", "/"],
  ]) {
    if (!(await navigation.isVisible()))
      await page.getByRole("button", { name: /Open navigation/ }).click();
    await navigation.getByRole("link", { name: label, exact: true }).click();

    if (path === "/portfolio") {
      await expect(page).toHaveURL(/\/login\?next=%2Fportfolio$/);
      await expect(navigation.locator('[aria-current="page"]')).toHaveCount(0);
    } else {
      await expect(page).toHaveURL(path);
      await expect(
        navigation.getByRole("link", {
          name: label,
          exact: true,
          includeHidden: true,
        }),
      ).toHaveAttribute("aria-current", "page");
    }

    const actual = await headerGeometry(page);
    actual.forEach((box, index) =>
      box.forEach((value, dimension) => {
        expect(
          Math.abs(value - original[index][dimension]),
          `${label}: header element ${index}, dimension ${dimension}`,
        ).toBeLessThan(0.25);
      }),
    );
  }

  for (const theme of ["light", "dark"]) {
    await page
      .getByRole("button", { name: `Switch to ${theme} theme` })
      .click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
    const actual = await headerGeometry(page);
    actual.forEach((box, index) =>
      box.forEach((value, dimension) => {
        expect(
          Math.abs(value - original[index][dimension]),
          `${theme}: header element ${index}, dimension ${dimension}`,
        ).toBeLessThan(0.25);
      }),
    );
  }
});
