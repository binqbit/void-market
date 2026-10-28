import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";
import type { AccountBid, BidActivity } from "../../src/account/types";

const now = Date.now();
function bid(index: number): AccountBid {
  return {
    auction_id: `auction-${index}`,
    asset_name: index === 0 ? "Dark Matter" : `Collectible ${index}`,
    amount: index === 0 ? "1.00000000" : "90071992.54740993",
    auction_status: index < 2 ? "open" : "completed",
    participation_status: "active",
    is_winner: index >= 2 && index <= 4,
    created_at_ms: now,
    closes_at_ms: now + 3_600_000,
    bidding_closes_at_ms: now,
    approvals:
      index === 0
        ? { requested: 6, approved: 4, pending: 2 }
        : index === 1
          ? { requested: 0, approved: 0, pending: 0 }
          : { requested: 5, approved: 5, pending: 0 },
  };
}

async function setup(page: Page) {
  const state = {
    failed: false,
    data: {
      stats: {
        bids_placed: 12,
        active_bids: 2,
        completed_purchases: 3,
        pending_approvals: 2,
      },
      bids: Array.from({ length: 12 }, (_, index) => bid(index)),
      has_more: true,
      server_time_ms: now,
    } satisfies BidActivity,
  };
  await page.addInitScript(() => {
    class Events extends EventTarget {
      close() {}
    }
    Object.defineProperty(window, "EventSource", { value: Events });
  });
  await page.route("**/api/v1/me", (route) =>
    route.fulfill({
      json: {
        address: "00000000-0000-4000-8000-000000000001",
        name: "Alice",
        username: "alice",
        is_demo: true,
      },
    }),
  );
  await page.route("**/api/v1/auth/config", (route) =>
    route.fulfill({ json: { google: true, demo: true } }),
  );
  await page.route("**/api/v1/messages", (route) =>
    route.fulfill({
      json: {
        custody_mode: "development",
        revision: 1,
        server_time_ms: now,
        pending: 0,
        messages: [],
        sessions: [],
        transactions: [],
      },
    }),
  );
  await page.route("**/api/v1/me/bids?*", (route) => {
    if (state.failed)
      return route.fulfill({
        status: 503,
        json: { message: "Bidding activity unavailable" },
      });
    const offset = Number(
      new URL(route.request().url()).searchParams.get("offset"),
    );
    return route.fulfill({
      json: {
        ...state.data,
        bids: state.data.bids.slice(offset, offset + 10),
        has_more: offset + 10 < state.data.bids.length,
      },
    });
  });
  return state;
}

test("profile shows private bid progress, exact amounts and global statistics across pages", async ({
  page,
}, info) => {
  await setup(page);
  await page.goto("/account");
  const section = page.getByRole("region", { name: "Your bidding" });
  await expect(section.locator("dl")).toContainText("12");
  const reveal = section.getByRole("button", { name: "My bids" });
  await expect(reveal).toHaveAttribute("aria-expanded", "false");
  await reveal.click();
  await expect(reveal).toHaveAttribute("aria-expanded", "true");
  const row = section.getByRole("listitem").filter({ hasText: "Dark Matter" });
  await expect(row).toContainText("1.0 ZEC");
  await expect(row).toContainText("4 of 6 requests approved");
  await expect(
    row.getByRole("link", { name: "Review 2 approvals" }),
  ).toHaveAttribute("href", "/messages");
  await expect(row.getByRole("link", { name: "Dark Matter" })).toHaveAttribute(
    "href",
    "/market/auction-0",
  );
  await expect(
    section.getByText("More may arrive", { exact: false }),
  ).toBeVisible();
  expect(
    await section.evaluate((node) => node.scrollWidth <= node.clientWidth),
  ).toBe(true);
  await page.screenshot({
    path: `${test.info().outputDir}/bidding-${info.project.name}.png`,
  });

  await section.getByRole("button", { name: "Next", exact: true }).click();
  await expect(
    section.getByText("Collectible 10", { exact: false }),
  ).toBeVisible();
  await expect(section.getByText("Dark Matter", { exact: false })).toHaveCount(
    0,
  );
  await expect(section.locator("dl")).toContainText("12");
  await expect(
    section.getByRole("button", { name: "Next", exact: true }),
  ).toBeDisabled();
  await section.getByRole("button", { name: "Previous", exact: true }).click();
  await expect(
    section.getByText("Dark Matter", { exact: false }),
  ).toBeVisible();
});

test("an unavailable bidding summary never becomes zero and can recover to an empty history", async ({
  page,
}) => {
  const state = await setup(page);
  state.failed = true;
  await page.goto("/account");
  const section = page.getByRole("region", { name: "Your bidding" });
  await expect(section.getByRole("alert")).toContainText(
    "Bidding activity unavailable",
  );
  await expect(section.locator("dl")).toHaveCount(0);
  state.failed = false;
  state.data.stats = {
    bids_placed: 0,
    active_bids: 0,
    completed_purchases: 0,
    pending_approvals: 0,
  };
  state.data.bids = [];
  await section.getByRole("button", { name: "Try again" }).click();
  await section.getByRole("button", { name: "My bids" }).click();
  await expect(
    section.getByText("You haven’t placed any bids yet."),
  ).toBeVisible();
  await expect(
    section.getByRole("link", { name: "Explore the market" }),
  ).toHaveAttribute("href", "/market");
});

test("new approval progress refreshes while an open profile keeps its context", async ({
  page,
}) => {
  const state = await setup(page);
  await page.goto("/account");
  const section = page.getByRole("region", { name: "Your bidding" });
  await section.getByRole("button", { name: "My bids" }).click();
  const row = section.getByRole("listitem").filter({ hasText: "Dark Matter" });
  await expect(row).toContainText("4 of 6 requests approved");
  state.data.bids[0].approvals = { requested: 7, approved: 6, pending: 1 };
  state.data.stats.pending_approvals = 1;
  await expect(row).toContainText("6 of 7 requests approved", {
    timeout: 8000,
  });
  await expect(
    row.getByRole("link", { name: "Review 1 approval", exact: true }),
  ).toBeVisible();
  await expect(
    section.getByRole("button", { name: "My bids" }),
  ).toHaveAttribute("aria-expanded", "true");
  state.failed = true;
  await expect(section.getByRole("alert")).toContainText(
    "Showing the last update",
    { timeout: 8000 },
  );
  await expect(row).toContainText("6 of 7 requests approved");
});
