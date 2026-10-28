import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";

async function setup(page: Page, signedIn = false) {
  const state = { signedIn };
  await page.addInitScript(() => {
    class Events extends EventTarget {
      close() {}
    }
    Object.defineProperty(window, "EventSource", { value: Events });
  });
  await page.route("**/api/v1/auth/config", (route) =>
    route.fulfill({ json: { google: true, demo: true } }),
  );
  await page.route("**/api/v1/me", (route) =>
    route.fulfill(
      state.signedIn
        ? {
            json: {
              address: "00000000-0000-4000-8000-000000000001",
              name: "Alice",
              username: "alice",
              email: null,
              is_demo: true,
            },
          }
        : { status: 401, json: { message: "Sign in" } },
    ),
  );
  await page.route("**/api/v1/messages", (route) =>
    route.fulfill({
      json: {
        custody_mode: "development",
        revision: 1,
        server_time_ms: Date.now(),
        pending: 0,
        messages: [],
        sessions: [],
      },
    }),
  );
  await page.route("**/api/v1/me/assets?*", (route) =>
    route.fulfill({ json: [] }),
  );
  await page.route("**/api/v1/me/transactions?*", (route) =>
    route.fulfill({ json: [] }),
  );
  await page.route("**/api/v1/auctions/auction-123", (route) =>
    route.fulfill({ status: 404, json: { message: "Fixture auction" } }),
  );
  await page.route("**/api/v1/demo/login", (route) => {
    state.signedIn = true;
    return route.fulfill({ json: { signed_in: true } });
  });
  return state;
}

test("sign-in preserves the intended auction and rejects external return destinations", async ({
  page,
}) => {
  await setup(page);
  await page.goto("/login?next=%2Fmarket%2Fauction-123");
  await expect(
    page.getByRole("link", { name: "Continue with Google" }),
  ).toHaveAttribute("href", /next=%2Fmarket%2Fauction-123/);
  await page.getByRole("button", { name: "Continue as Alice" }).click();
  await expect(page).toHaveURL(/\/market\/auction-123$/);

  await page.goto("/login?next=https%3A%2F%2Fexample.org");
  await expect(page).toHaveURL(/\/portfolio$/);
});

test("mobile navigation exposes every destination and closes with Escape", async ({
  page,
}, info) => {
  await setup(page, true);
  await page.goto("/portfolio");
  if (info.project.name === "mobile") {
    const button = page.getByRole("button", { name: /Open navigation/ });
    await button.click();
    const nav = page.getByRole("navigation", { name: "Main navigation" });
    await expect(
      nav.getByRole("link", { name: "Home", exact: true }),
    ).toBeFocused();
    for (const name of [
      "Home",
      "Market",
      "My portfolio",
      "Messages",
      "Docs",
      "Demo",
    ]) {
      await expect(nav.getByRole("link", { name, exact: true })).toBeVisible();
    }
    await page.keyboard.press("Escape");
    await expect(nav).toBeHidden();
    await expect(button).toBeFocused();
  }
  await page.getByRole("tab", { name: "My assets", exact: true }).focus();
  await page.keyboard.press("ArrowRight");
  await expect(
    page.getByRole("tab", { name: "History", exact: true }),
  ).toBeFocused();
  await expect(
    page.getByRole("tab", { name: "History", exact: true }),
  ).toHaveAttribute("aria-selected", "true");
});

test("privacy links reach their section and demo accounts have accurate sign-in labels", async ({
  page,
}) => {
  await setup(page, true);
  await page.goto("/");
  await page.getByRole("link", { name: "Understand privacy" }).click();
  await expect(page).toHaveURL(/\/docs#privacy$/);
  await expect(page.locator("#privacy")).toBeInViewport();
  await expect(page.locator("#privacy")).toBeFocused();
  await page.goto("/account");
  await expect(
    page.getByText("Signed in with a local demo account."),
  ).toBeVisible();
  await expect(page.getByText("Signed in with Google.")).toHaveCount(0);
});

test("an unavailable inbox never claims there are no requests", async ({
  page,
}) => {
  await setup(page, true);
  await page.route("**/api/v1/messages", (route) =>
    route.fulfill({ status: 503, json: { message: "Inbox unavailable" } }),
  );
  await page.goto("/messages");
  await expect(page.getByText("Inbox unavailable")).toBeVisible();
  await expect(page.getByText("You are all caught up")).toHaveCount(0);
  await expect(page.getByText("0 awaiting approval")).toHaveCount(0);
  await expect(page.getByText("Requests unavailable")).toBeVisible();
});
