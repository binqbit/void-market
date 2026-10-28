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
  await page.route("**/api/v1/me/bids?*", (route) =>
    route.fulfill({
      json: {
        stats: {
          bids_placed: 0,
          active_bids: 0,
          completed_purchases: 0,
          pending_approvals: 0,
        },
        bids: [],
        has_more: false,
        server_time_ms: Date.now(),
      },
    }),
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

test("guests can browse Market without a sign-in error or redirect", async ({
  page,
}) => {
  await setup(page);
  await page.route("**/api/v1/auctions?*", (route) =>
    route.fulfill({ json: [] }),
  );
  await page.goto("/market");
  await expect(page.getByRole("heading", { name: "The market" })).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "No open auctions." }),
  ).toBeVisible();
  await expect(page).toHaveURL(/\/market$/);
  await expect(page.getByRole("alert")).toHaveCount(0);
});

for (const { destination, prompt, heading } of [
  {
    destination: "/portfolio?view=assets#holdings",
    prompt: "Sign in to view your portfolio and auction activity.",
    heading: "My portfolio",
  },
  {
    destination: "/messages",
    prompt: "Sign in to view your messages and approvals.",
    heading: "Messages",
  },
  {
    destination: "/account",
    prompt: "Sign in to manage your profile.",
    heading: "Your account",
  },
]) {
  test(`${destination} explains sign-in and returns there after authentication`, async ({
    page,
  }) => {
    await setup(page);
    const privateRequests: string[] = [];
    page.on("request", (request) => {
      if (/\/api\/v1\/(me\/|messages)/.test(request.url())) {
        privateRequests.push(request.url());
      }
    });

    await page.goto(destination);
    await expect(page).toHaveURL(/\/login\?/);
    expect(new URL(page.url()).searchParams.get("next")).toBe(destination);
    const notice = page.getByRole("status").filter({ hasText: prompt });
    await expect(notice).toBeVisible();
    await expect(notice).not.toHaveClass(/error/);
    await expect(page.getByRole("alert")).toHaveCount(0);
    await expect(
      page.getByRole("link", { name: "Continue with Google" }),
    ).toHaveAttribute(
      "href",
      `/api/v1/auth/google/start?next=${encodeURIComponent(destination)}`,
    );
    expect(privateRequests).toEqual([]);

    await page.getByRole("button", { name: "Continue as Alice" }).click();
    await expect(page).toHaveURL(
      new URL(destination, "http://localhost:3000").href,
    );
    await expect(
      page.getByRole("heading", { level: 1, name: heading }),
    ).toBeVisible();
  });

  test(`${destination} keeps genuine session failures visible and retryable`, async ({
    page,
  }) => {
    await setup(page, true);
    await page.route("**/api/v1/me", (route) =>
      route.fulfill({ status: 503, json: { message: "Account unavailable" } }),
    );
    await page.goto(destination);
    await expect(
      page.getByRole("alert").filter({ hasText: "Account unavailable" }),
    ).toBeVisible();
    await expect(page).toHaveURL(
      new URL(destination, "http://localhost:3000").href,
    );

    await page.unroute("**/api/v1/me");
    await page.route("**/api/v1/me", (route) =>
      route.fulfill({ status: 401, json: { message: "Sign in first" } }),
    );
    await page.getByRole("button", { name: "Try again", exact: true }).click();
    await expect(
      page.getByRole("status").filter({ hasText: prompt }),
    ).toBeVisible();
    await expect(page.getByRole("alert")).toHaveCount(0);
  });
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
