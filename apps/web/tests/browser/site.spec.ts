import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.route("**/api/v1/auth/config", (route) =>
    route.fulfill({ json: { google: false, demo: false } }),
  );
  await page.route("**/api/v1/messages", (route) =>
    route.fulfill({
      json: {
        custody_mode: "disabled",
        revision: 0,
        server_time_ms: Date.now(),
        pending: 0,
        messages: [],
        sessions: [],
      },
    }),
  );
  await page.route("**/api/v1/messages/events", (route) =>
    route.fulfill({
      contentType: "text/event-stream",
      body: 'event: messages\ndata: {"revision":0}\n\n',
    }),
  );
});

const address = "00000000-0000-4000-8000-000000000001";
const recipient = "00000000-0000-4000-8000-000000000002";
const assetId = "00000000-0000-4000-8000-000000000003";
const profile = {
  address,
  name: "Test owner",
  username: "test_owner",
  email: "owner@example.test",
};
const asset = {
  id: assetId,
  name: "A real asset record",
  description: "Description returned by the API",
  image: null,
  owner_id: address,
  created_at: "2026-10-28T12:00:00Z",
};

async function publicApi(page: Page) {
  await page.route("**/api/v1/auctions?*", (route) =>
    route.fulfill({ json: [] }),
  );
  await page.route("**/api/v1/me", (route) =>
    route.fulfill({ status: 401, json: { message: "Sign in first" } }),
  );
  await page.route("**/api/v1/auth/config", (route) =>
    route.fulfill({ json: { google: false } }),
  );
}

test("navigation, dark default and explicit theme survive reload without overflow", async ({
  page,
}) => {
  await publicApi(page);
  await page.emulateMedia({ colorScheme: "light", reducedMotion: "reduce" });
  await page.goto("/");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await expect(page.getByRole("heading", { level: 1 })).toContainText(
    "Trade privately,",
  );
  await expect(
    page.getByRole("button", { name: /Switch to .+ theme/ }),
  ).toHaveCount(1);
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.emulateMedia({ colorScheme: "light" });
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page
    .getByRole("button", { name: "Switch to light theme", exact: true })
    .click();
  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await page
    .getByRole("button", { name: "Switch to dark theme", exact: true })
    .click();
  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");

  for (const [link, heading] of [
    ["Market", "The market"],
    ["My portfolio", "A space for your assets."],
    ["Docs", "Understand Void Market"],
  ]) {
    if (!(await page.locator("#main-navigation").isVisible()))
      await page.getByRole("button", { name: /Open navigation/ }).click();
    await page
      .getByRole("navigation", { name: "Main navigation" })
      .getByRole("link", { name: link, exact: true })
      .click();
    await expect(
      page.getByRole("heading", { level: 1, name: heading, exact: true }),
    ).toBeVisible();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= window.innerWidth,
      ),
    ).toBe(true);
  }
  await page.goto("/login");
  await expect(
    page.getByText("Google sign-in is being configured."),
  ).toBeVisible();
  await expect(page.getByRole("textbox")).toHaveCount(0);
});

test("portfolio shows loading failures, real metadata and image fallback", async ({
  page,
}) => {
  await page.route("**/api/v1/me", (route) => route.fulfill({ json: profile }));
  await page.route("**/api/v1/me/assets?*", (route) =>
    route.fulfill({
      status: 503,
      json: { message: "Temporarily unavailable" },
    }),
  );
  await page.goto("/portfolio");
  await expect(page.getByRole("alert")).toContainText(
    "Temporarily unavailable",
  );
  await expect(page.getByText("A little room for possibility.")).toHaveCount(0);
  await page.route("**/api/v1/me/assets?*", (route) =>
    route.fulfill({ json: [asset] }),
  );
  await page.getByRole("button", { name: "Try again" }).click();
  await expect(page.getByRole("heading", { name: asset.name })).toBeVisible();
  await expect(page.getByText("Image unavailable")).toBeVisible();
  await expect(page.getByText(asset.description)).toBeVisible();
});

test("an expired session returns the portfolio to sign-in", async ({
  page,
}) => {
  await page.route("**/api/v1/me", (route) => route.fulfill({ json: profile }));
  await page.route("**/api/v1/me/assets?*", (route) =>
    route.fulfill({ status: 401, json: { message: "Sign in first" } }),
  );
  await page.goto("/portfolio");
  await expect(page).toHaveURL(/\/login\?next=%2Fportfolio$/);
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Sign in to view your portfolio" }),
  ).toBeVisible();
  await expect(page.getByRole("alert")).toHaveCount(0);
});

test("a late profile response cannot restore an expired session", async ({
  page,
}) => {
  let releaseProfile: () => void = () => {};
  const delayedProfile = new Promise<void>((resolve) => {
    releaseProfile = resolve;
  });
  let requests = 0;
  await page.route("**/api/v1/me", async (route) => {
    requests += 1;
    if (requests === 1) await delayedProfile;
    await route.fulfill({ json: profile });
  });
  await page.route("**/api/v1/me/assets?*", (route) =>
    route.fulfill({ status: 401, json: { message: "Sign in first" } }),
  );

  await page.goto("/portfolio");
  const signedOut = page.getByRole("status").filter({
    hasText: "Sign in to view your portfolio",
  });
  await expect(signedOut).toBeVisible();

  const lateResponse = page.waitForResponse("**/api/v1/me");
  releaseProfile();
  await lateResponse;
  await page.waitForLoadState("networkidle");
  await expect(signedOut).toBeVisible();
  await expect(page).toHaveURL(/\/login\?next=%2Fportfolio$/);
  await expect(page.getByRole("alert")).toHaveCount(0);
});

test("transfer confirmation and recovery reuse the same request after a lost response", async ({
  page,
}) => {
  await page.route("**/api/v1/me", (route) => route.fulfill({ json: profile }));
  await page.route("**/api/v1/me/assets?*", (route) =>
    route.fulfill({ json: [asset] }),
  );
  await page.route(`**/api/v1/assets/${assetId}`, (route) =>
    route.fulfill({ json: asset }),
  );
  await page.route(`**/api/v1/accounts/${recipient}`, (route) =>
    route.fulfill({
      json: { address: recipient, name: "Recipient", username: "recipient" },
    }),
  );
  let requestId = "";
  let submissions = 0;
  await page.route(`**/api/v1/assets/${assetId}/transfers`, async (route) => {
    const body = route.request().postDataJSON() as { request_id: string };
    submissions += 1;
    if (!requestId) requestId = body.request_id;
    expect(body.request_id).toBe(requestId);
    if (submissions === 1) await route.abort("failed");
    else
      await route.fulfill({
        json: {
          id: requestId,
          asset_id: assetId,
          sender_id: address,
          recipient_id: recipient,
          created_at: "2026-10-28T12:00:00Z",
        },
      });
  });
  await page.route("**/api/v1/transfers/*", (route) =>
    route.fulfill({ status: 404, json: { message: "Not found" } }),
  );
  await page.goto("/portfolio");
  await page.getByRole("button", { name: "Transfer", exact: true }).click();
  await page.getByLabel("Recipient account address").fill(recipient);
  await page.getByRole("button", { name: "Find recipient" }).click();
  await expect(page.getByRole("dialog")).toContainText("@recipient");
  await expect(page.getByRole("dialog")).toContainText(asset.name);
  await page.getByRole("button", { name: "Confirm transfer" }).click();
  await expect(page.getByRole("alert")).toContainText(
    "outcome is not confirmed",
  );
  await page.getByRole("button", { name: "Close dialog" }).click();
  await page.reload();
  await page.getByRole("button", { name: "Review transfer" }).click();
  await page.getByRole("button", { name: "Check transfer receipt" }).click();
  await page.getByRole("button", { name: "Retry the same transfer" }).click();
  await expect(
    page.getByRole("heading", { name: "Transfer complete" }),
  ).toBeVisible();
  expect(submissions).toBe(2);
  expect(
    await page.evaluate(() => sessionStorage.getItem("void.pending-transfer")),
  ).toBeNull();
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await page.getByRole("button", { name: "Add an asset" }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "Add an asset" }),
  ).toBeFocused();
});
