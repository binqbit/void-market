import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";
import type { Auction } from "../../src/auctions/types";

const seller = {
  address: "00000000-0000-4000-8000-000000000001",
  name: "Seller",
  username: "seller",
};
const buyer = {
  address: "00000000-0000-4000-8000-000000000002",
  name: "Buyer",
  username: "buyer",
};
const id = "00000000-0000-4000-8000-000000000010";
const assetId = "00000000-0000-4000-8000-000000000020";
const completion = "00000000-0000-4000-8000-000000000030";

function auction(): Auction {
  return {
    id,
    asset: {
      id: assetId,
      name: "The quiet signal",
      description: "A collectible with a real description.",
      image: null,
    },
    seller,
    winner: null,
    status: "open",
    reason: null,
    currency: "ZEC",
    execution: "development_custody",
    payment_status: "unverified",
    wallet_ready: true,
    closes_at_ms: Date.now() + 86400000,
    bidding_closes_at_ms: Date.now() + 86400000 - 3600000,
    created_at_ms: Date.now(),
    published_at_ms: Date.now(),
    finished_at_ms: null,
    revision: 7,
    policy_hash: "fixture-policy",
    buyer_limit: 3,
    admitted_buyers: 0,
    active_buyers: 0,
    generation: 0,
    checks_completed: 0,
    checks_total: 0,
    reserve: null,
    your_bid: null,
    agreed_price: null,
    participation: null,
    completion_request_id: null,
    actions: {
      edit: false,
      publish: false,
      cancel: false,
      bid: true,
      withdraw: false,
      complete: false,
      close_bidding: false,
    },
  };
}

test("demo seller confirms early closure and keeps the remaining approval flow", async ({
  page,
}, info) => {
  const data = auction();
  data.execution = "localnet_custody";
  data.actions.bid = false;
  data.actions.close_bidding = true;
  data.active_buyers = 2;
  data.participation = {
    status: "active",
    remaining_ms: 600000,
    timer_running: false,
    created_at_ms: Date.now(),
  };
  await fixture(page, "seller", data);
  let calls = 0;
  await page.route(`**/api/v1/auctions/${id}/close-bidding`, (route) => {
    calls += 1;
    data.closes_at_ms = Date.now() + 3600000;
    data.bidding_closes_at_ms = data.closes_at_ms - 3600000;
    data.required_from_ms = data.bidding_closes_at_ms;
    data.status = "matching";
    data.actions.close_bidding = false;
    return route.fulfill({ json: data });
  });
  await page.goto(`/market/${id}`);
  await page.getByRole("button", { name: "Close bidding now" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("Approvals already collected are kept");
  await expect(dialog).toContainText("10-minute");
  await page.screenshot({
    path: info.outputPath("close-bidding-review.png"),
    fullPage: true,
  });
  expect(calls).toBe(0);
  await dialog.getByRole("button", { name: "Keep bidding open" }).click();
  expect(calls).toBe(0);
  await page.getByRole("button", { name: "Close bidding now" }).click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Close bidding now" })
    .click();
  await expect(dialog).toHaveCount(0);
  await expect(
    page.getByText("Bidding is closed. Finish your remaining approvals", {
      exact: false,
    }),
  ).toBeVisible();
  await expect(page.getByRole("link", { name: "Open Messages" })).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Close bidding now" }),
  ).toHaveCount(0);
  expect(calls).toBe(1);
});

test("ordinary auctions do not expose demo acceleration", async ({ page }) => {
  await fixture(page, "seller", auction());
  await page.goto(`/market/${id}`);
  await expect(
    page.getByRole("button", { name: "Close bidding now" }),
  ).toHaveCount(0);
});

test("native auctions keep payment progress without a technical status panel", async ({
  page,
}) => {
  const data = auction();
  data.execution = "localnet_custody";
  data.actions.bid = false;
  data.custody_status = "held";
  await fixture(page, "seller", data);
  await page.goto(`/market/${id}`);
  await expect(
    page.getByRole("heading", { name: data.asset.name, level: 1 }),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Local settlement" }),
  ).toHaveCount(0);
  await expect(page.getByText("NFT: held", { exact: true })).toHaveCount(0);
  await expect(
    page.getByText("Payment: unverified", { exact: true }),
  ).toHaveCount(0);

  data.status = "awaiting_completion";
  data.winner = buyer;
  data.agreed_price = "1.50000000";
  data.payment_status = "pending";
  data.payment = {
    expected: "1.50000000",
    paid: "0.50000000",
    remaining: "1.00000000",
  };
  data.settlement_note = "A top-up is required to complete payment";
  await page.reload();
  await expect(
    page.getByText("Paid 0.5 / 1.5 ZEC. Remaining: 1.0 ZEC.", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText(data.settlement_note, { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Local settlement" }),
  ).toHaveCount(0);
});

async function fixture(
  page: Page,
  role: "seller" | "buyer" | "public",
  data: Auction,
) {
  await page.addInitScript(() => {
    class Events extends EventTarget {
      close() {}
    }
    Object.defineProperty(window, "EventSource", { value: Events });
  });
  await page.route("**/api/v1/me", (route) =>
    role === "public"
      ? route.fulfill({ status: 401, json: { message: "Sign in first" } })
      : route.fulfill({
          json: {
            ...(role === "seller" ? seller : buyer),
            email: "fixture@example.test",
          },
        }),
  );
  const snapshot = {
    custody_mode: "development",
    revision: 1,
    server_time_ms: Date.now(),
    pending: 0,
    messages: [],
    sessions: [],
    sales: [] as unknown[],
  };
  await page.route("**/api/v1/messages", (route) =>
    route.fulfill({ json: snapshot }),
  );
  await page.route("**/api/v1/auctions?*", (route) =>
    route.fulfill({ json: [data] }),
  );
  await page.route(`**/api/v1/auctions/${id}`, (route) =>
    route.fulfill({ json: data }),
  );
  await page.route("**/api/v1/me/assets?*", (route) =>
    route.fulfill({
      json: [
        {
          ...data.asset,
          owner_id: seller.address,
          active_auction_id: null,
          created_at: new Date().toISOString(),
        },
      ],
    }),
  );
  await page.route("**/api/v1/me/auctions?*", (route) =>
    route.fulfill({ json: [data] }),
  );
  return snapshot;
}

test("catalog and public auction show rules without private prices", async ({
  page,
}, info) => {
  const data = auction();
  data.actions.bid = false;
  await fixture(page, "public", data);
  await page.goto("/market");
  await expect(
    page.getByRole("heading", { name: data.asset.name }),
  ).toBeVisible();
  await page.getByRole("link", { name: "View auction", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: data.asset.name, level: 1 }),
  ).toBeVisible();
  await expect(
    page.getByRole("link", { name: "Sign in to bid" }),
  ).toBeVisible();
  await expect(page.getByText("Your private reserve")).toHaveCount(0);
  await page.getByText("Auction rules", { exact: true }).click();
  await expect(
    page.getByText("Equal bids favor the earlier admitted bid."),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.evaluate(() => window.scrollTo({ top: 0, behavior: "instant" }));
  await page.screenshot({
    path: info.outputPath("auction-public.png"),
    fullPage: true,
  });
});

test("closed bidding leaves the public market and stays in participant history", async ({
  page,
}) => {
  const data = auction();
  data.your_bid = "2.00000000";
  data.participation = {
    status: "active",
    remaining_ms: 600000,
    timer_running: false,
    created_at_ms: Date.now(),
  };
  await fixture(page, "buyer", data);
  await page.goto("/market");
  await expect(
    page.getByRole("heading", { name: data.asset.name }),
  ).toBeVisible();

  data.status = "matching";
  data.actions.bid = false;
  data.bidding_closes_at_ms = Date.now() - 1;
  await page.route("**/api/v1/auctions?*", (route) =>
    route.fulfill({ json: [] }),
  );
  await expect(
    page.getByRole("heading", { name: "No open auctions." }),
  ).toBeVisible({ timeout: 8000 });
  await expect(
    page.getByRole("heading", { name: data.asset.name }),
  ).toHaveCount(0);

  await page.goto("/portfolio");
  await page.getByRole("tab", { name: "My auctions", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: data.asset.name }),
  ).toBeVisible();
  await page.getByRole("link", { name: "View auction", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: data.asset.name, level: 1 }),
  ).toBeVisible();
  await expect(
    page.getByRole("link", { name: "Back to my portfolio" }),
  ).toHaveAttribute("href", "/portfolio");
});

test("portfolio explains why an auction ended without a sale", async ({
  page,
}) => {
  const data = auction();
  data.status = "no_sale";
  data.reason = "reserve_not_met";
  data.actions.bid = false;
  data.your_bid = "0.67000000";
  data.participation = {
    status: "active",
    remaining_ms: 600000,
    timer_running: false,
    created_at_ms: Date.now(),
  };
  await fixture(page, "buyer", data);
  await page.goto("/portfolio");
  await page.getByRole("tab", { name: "My auctions" }).click();
  const card = page.locator("article").filter({ hasText: data.asset.name });
  await expect(card).toContainText(
    "The auction ended without a sale because no eligible bid met the private reserve.",
  );
  await card.getByRole("link", { name: "View auction" }).click();
  await expect(page.locator(".auction-summary")).toContainText(
    "The auction ended without a sale because no eligible bid met the private reserve.",
  );
  await expect(page.getByText("Your private reserve")).toHaveCount(0);
});

test("an outsider loses cached auction details when bidding closes", async ({
  page,
}) => {
  const data = auction();
  await fixture(page, "public", data);
  await page.goto(`/market/${id}`);
  await expect(
    page.getByRole("heading", { name: data.asset.name, level: 1 }),
  ).toBeVisible();
  await page.route(`**/api/v1/auctions/${id}`, (route) =>
    route.fulfill({
      status: 404,
      json: { message: "Resource was not found" },
    }),
  );
  await expect(page.getByRole("alert")).toContainText(
    "Resource was not found",
    { timeout: 8000 },
  );
  await expect(
    page.getByRole("heading", { name: data.asset.name, level: 1 }),
  ).toHaveCount(0);
  await expect(page.getByRole("link", { name: "Sign in to bid" })).toHaveCount(
    0,
  );
});

test("bid review preserves the exact decimal string and retry identity", async ({
  page,
}) => {
  const data = auction();
  await fixture(page, "buyer", data);
  const requests: unknown[] = [];
  await page.route(`**/api/v1/auctions/${id}/bids`, async (route) => {
    requests.push(route.request().postDataJSON());
    if (requests.length === 1) {
      await route.fulfill({
        status: 503,
        json: { message: "Temporarily unavailable" },
      });
      return;
    }
    data.your_bid = "90071992.54740993";
    data.actions.bid = false;
    data.actions.withdraw = true;
    data.participation = {
      status: "active",
      remaining_ms: 600000,
      timer_running: false,
      created_at_ms: Date.now(),
    };
    await route.fulfill({ status: 202, json: data });
  });
  await page.goto(`/market/${id}`);
  await page
    .getByLabel("Your bid · ZEC", { exact: true })
    .fill("90071992.54740993");
  await page.getByRole("button", { name: "Review bid" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("90071992.54740993 ZEC");
  await dialog.getByRole("button", { name: "Confirm bid" }).click();
  await expect(dialog.getByRole("alert")).toContainText(
    "Temporarily unavailable",
  );
  await dialog.getByRole("button", { name: "Confirm bid" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Review bid" })).toHaveCount(0);
  expect(requests).toHaveLength(2);
  expect(requests[0]).toEqual(requests[1]);
  expect((requests[1] as { amount: string }).amount).toBe("90071992.54740993");
});

test("portfolio draft requires explicit publish consent", async ({ page }) => {
  const data = auction();
  data.status = "draft";
  data.reserve = "1.25000000";
  data.published_at_ms = null;
  data.actions = {
    edit: true,
    publish: true,
    cancel: true,
    bid: false,
    withdraw: false,
    complete: false,
    close_bidding: false,
  };
  await fixture(page, "seller", data);
  await page.route("**/api/v1/auctions", async (route) => {
    const body = route.request().postDataJSON();
    expect(body.reserve).toBe("1.25000000");
    data.closes_at_ms = body.closes_at_ms;
    await route.fulfill({ status: 201, json: data });
  });
  let consent: unknown = null;
  await page.route(`**/api/v1/auctions/${id}/publish`, async (route) => {
    consent = route.request().postDataJSON();
    data.status = "open";
    data.actions = {
      edit: false,
      publish: false,
      cancel: false,
      bid: false,
      withdraw: false,
      complete: false,
      close_bidding: false,
    };
    await route.fulfill({ json: data });
  });
  await page.goto("/portfolio");
  await page.getByRole("button", { name: "Sell asset", exact: true }).click();
  await page
    .getByLabel("Private reserve price · ZEC", { exact: true })
    .fill("1.25000000");
  await page.getByRole("button", { name: "Save draft" }).click();
  await expect(page).toHaveURL(new RegExp(`/market/${id}$`));
  await expect(
    page.getByLabel("Private reserve price · ZEC", { exact: true }),
  ).toHaveValue("1.25");
  await expect(
    page.getByRole("button", { name: "Save changes" }),
  ).toBeDisabled();
  const publish = page.getByRole("button", { name: "Publish auction" });
  await expect(publish).toBeDisabled();
  await page.getByRole("checkbox").check();
  await publish.click();
  await expect(page.getByText("Open for bids", { exact: true })).toBeVisible();
  expect(consent).toEqual({ revision: 7, consent: true });
});

test("draft amounts stay compact without changing saved terms or precision", async ({
  page,
}) => {
  const data = auction();
  data.status = "draft";
  data.reserve = "1.00000000";
  data.published_at_ms = null;
  data.actions.edit = true;
  data.actions.publish = true;
  data.actions.cancel = true;
  data.actions.bid = false;
  await fixture(page, "seller", data);
  const saved: string[] = [];
  await page.route(`**/api/v1/auctions/${id}/draft`, async (route) => {
    const body = route.request().postDataJSON();
    saved.push(body.reserve);
    data.reserve = body.reserve === "2.5" ? "2.50000000" : body.reserve;
    data.revision += 1;
    await route.fulfill({ json: data });
  });

  await page.goto(`/market/${id}`);
  const reserve = page.getByLabel("Private reserve price · ZEC", {
    exact: true,
  });
  const save = page.getByRole("button", { name: "Save changes" });
  await expect(reserve).toHaveValue("1.0");
  await expect(save).toBeDisabled();
  await reserve.fill("1.00000000");
  await expect(save).toBeDisabled();
  await expect(page.getByRole("checkbox")).toBeEnabled();

  await reserve.fill("90071992.54740993");
  await save.click();
  await expect(save).toBeDisabled();
  await expect(reserve).toHaveValue("90071992.54740993");
  await reserve.fill("2.5");
  await save.click();
  await expect(reserve).toHaveValue("2.5");
  await expect(save).toBeDisabled();
  await expect(page.getByRole("checkbox")).toBeEnabled();
  expect(saved).toEqual(["90071992.54740993", "2.5"]);
});

test("seller completes from Messages with payment explicitly unverified", async ({
  page,
}, info) => {
  const data = auction();
  data.status = "awaiting_completion";
  data.winner = buyer;
  data.agreed_price = "1.25000000";
  data.generation = 1;
  data.completion_request_id = completion;
  data.actions = {
    edit: false,
    publish: false,
    cancel: false,
    bid: false,
    withdraw: false,
    complete: true,
    close_bidding: false,
  };
  const snapshot = await fixture(page, "seller", data);
  snapshot.pending = 1;
  snapshot.sales = [
    {
      auction_id: id,
      request_id: completion,
      generation: 1,
      title: data.asset.name,
      price: "1.25000000",
      buyer,
      remaining_ms: 420000,
      timer_running: true,
    },
  ];
  let request: unknown = null;
  await page.route(`**/api/v1/auctions/${id}/complete`, async (route) => {
    request = route.request().postDataJSON();
    snapshot.pending = 0;
    snapshot.sales = [];
    await route.fulfill({
      json: { payment_status: "unverified", transfer_id: "fixture" },
    });
  });
  await page.goto("/messages");
  await expect(
    page.getByRole("heading", { name: "Approve the sale transfer" }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: info.outputPath("sale-message.png"),
    fullPage: true,
  });
  await page.getByRole("button", { name: "Complete sale" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText(
    "Payment has not been processed or verified.",
  );
  const approve = dialog.getByRole("button", { name: "Approve & transfer" });
  await expect(approve).toBeDisabled();
  await dialog.getByRole("checkbox").check();
  await approve.click();
  await expect(
    page.getByRole("heading", { name: "You are all caught up" }),
  ).toBeVisible();
  expect(request).toEqual({ request_id: completion, generation: 1 });
});

test("a changed winner requires fresh transfer consent", async ({ page }) => {
  await page.clock.install();
  const data = auction();
  Object.assign(data, {
    status: "awaiting_completion",
    winner: buyer,
    agreed_price: "2.00000000",
    generation: 1,
    completion_request_id: completion,
  });
  data.actions.bid = false;
  data.actions.complete = true;
  await fixture(page, "seller", data);
  let submitted: unknown = null;
  await page.route(`**/api/v1/auctions/${id}/complete`, async (route) => {
    submitted = route.request().postDataJSON();
    data.status = "completed";
    data.actions.complete = false;
    await route.fulfill({ json: { payment_status: "unverified" } });
  });

  await page.goto(`/market/${id}`);
  await page.getByRole("button", { name: "Complete sale" }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("checkbox").check();
  const approve = dialog.getByRole("button", { name: "Approve & transfer" });
  await expect(approve).toBeEnabled();

  const nextRequest = "00000000-0000-4000-8000-000000000031";
  data.winner = { ...buyer, name: "Remaining buyer", username: "survivor" };
  data.agreed_price = "1.25000000";
  data.generation = 2;
  data.completion_request_id = nextRequest;
  await page.clock.runFor(5001);
  await expect(dialog).toContainText("@survivor");
  await expect(dialog).toContainText("1.25 ZEC");
  await expect(dialog.getByRole("checkbox")).not.toBeChecked();
  await expect(approve).toBeDisabled();
  expect(submitted).toBeNull();

  await dialog.getByRole("checkbox").check();
  await approve.click();
  await expect(dialog).toHaveCount(0);
  expect(submitted).toEqual({ request_id: nextRequest, generation: 2 });
});

test("withdrawal explains no reentry and updates the authoritative outcome", async ({
  page,
}) => {
  const data = auction();
  data.status = "matching";
  data.generation = 2;
  data.checks_total = 1;
  data.your_bid = "1.00000001";
  data.actions.bid = false;
  data.actions.withdraw = true;
  data.participation = {
    status: "active",
    remaining_ms: 123456,
    timer_running: false,
    created_at_ms: Date.now(),
  };
  await fixture(page, "buyer", data);
  await page.route(`**/api/v1/auctions/${id}/withdraw`, async (route) => {
    data.status = "no_sale";
    data.reason = "no_remaining_buyers";
    data.actions.withdraw = false;
    data.participation!.status = "withdrawn";
    await route.fulfill({ json: { mode: "dev_custodial" } });
  });
  await page.goto(`/market/${id}`);
  await page.getByRole("button", { name: "Withdraw bid", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("cannot bid again");
  await dialog
    .getByRole("button", { name: "Withdraw bid", exact: true })
    .click();
  await expect(
    page.getByText(
      "The auction ended without a sale because no participating buyers remain.",
    ),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: "Review bid" })).toHaveCount(0);
});
