import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";
import type { MessageSnapshot } from "../../src/messages/types";

const account = "00000000-0000-4000-8000-000000000001";
const sessionId = "00000000-0000-4000-8000-000000000010";
const messageId = "00000000-0000-4000-8000-000000000020";
const requestId = "00000000-0000-4000-8000-000000000030";

async function fixture(page: Page) {
  const snapshot: MessageSnapshot = {
    custody_mode: "development",
    revision: 1,
    server_time_ms: Date.now(),
    pending: 1,
    sessions: [
      {
        id: sessionId,
        title: "Private approval session",
        status: "active",
        participation_status: "active",
        remaining_ms: 600000,
        timer_running: true,
      },
    ],
    messages: [
      {
        id: messageId,
        session_id: sessionId,
        session_title: "Private approval session",
        created_at_ms: 30,
        request_id: requestId,
        revision: 1,
        stage: "mask_contribution",
        status: "ready",
        participant_status: "active",
        session_status: "active",
        remaining_ms: 600000,
        timer_running: true,
        approval: null,
      },
    ],
  };
  await page.addInitScript(() => {
    class Events extends EventTarget {
      onopen: (() => void) | null = null;
      private hint = () => this.dispatchEvent(new Event("messages"));
      constructor() {
        super();
        window.addEventListener("test-messages-hint", this.hint);
      }
      close() {
        window.removeEventListener("test-messages-hint", this.hint);
      }
    }
    Object.defineProperty(window, "EventSource", { value: Events });
  });
  await page.route("**/api/v1/me", (route) =>
    route.fulfill({
      json: {
        address: account,
        name: "Test owner",
        username: "test_owner",
        email: "owner@example.test",
      },
    }),
  );
  await page.route("**/api/v1/messages", (route) =>
    route.fulfill({ json: snapshot }),
  );
  return snapshot;
}

test("messages show one clear approval and preserve the layout", async ({
  page,
}, testInfo) => {
  const snapshot = await fixture(page);
  const requests: unknown[] = [];
  await page.route(`**/api/v1/messages/${messageId}/approve`, async (route) => {
    requests.push(route.request().postDataJSON());
    snapshot.messages[0].status = "approved";
    snapshot.messages[0].timer_running = false;
    snapshot.pending = 0;
    await route.fulfill({
      status: 202,
      json: { mode: "dev_custodial", signature: "fixture" },
    });
  });
  await page.goto("/messages");
  await expect(
    page.getByRole("heading", { name: "Messages", exact: true }),
  ).toBeVisible();
  await expect(page.getByText("Preparation", { exact: true })).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Your sessions" }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "Approve", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Reject", exact: true }),
  ).toHaveCount(0);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: testInfo.outputPath("messages.png"),
    fullPage: true,
  });
  await page.getByRole("button", { name: "Approve", exact: true }).click();
  await page.locator(".message-archive > summary").click();
  await expect(page.getByText("Approved — processing")).toBeVisible();
  expect(requests).toEqual([{ request_id: requestId, revision: 1 }]);
  await expect(
    page.getByRole("button", { name: "Approve", exact: true }),
  ).toHaveCount(0);
});

test("an update arriving during snapshot loading is not lost", async ({
  page,
}) => {
  const snapshot = await fixture(page);
  const stale = structuredClone(snapshot);
  let release = () => {};
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  let requests = 0;
  await page.route("**/api/v1/messages", async (route) => {
    requests += 1;
    if (requests === 1) {
      await held;
      await route.fulfill({ json: stale });
    } else {
      await route.fulfill({ json: snapshot });
    }
  });

  await page.goto("/messages");
  await expect.poll(() => requests).toBe(1);
  snapshot.messages[0].status = "approved";
  snapshot.messages[0].timer_running = false;
  snapshot.pending = 0;
  await page.evaluate(() =>
    window.dispatchEvent(new Event("test-messages-hint")),
  );
  release();

  await page.locator(".message-archive > summary").click();
  await expect(page.getByText("Approved — processing")).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Approve", exact: true }),
  ).toHaveCount(0);
  expect(requests).toBe(2);
});

test("withdrawal needs explicit confirmation and closes the session", async ({
  page,
}) => {
  const snapshot = await fixture(page);
  let withdrawals = 0;
  await page.route(
    `**/api/v1/approval-sessions/${sessionId}/withdraw`,
    async (route) => {
      withdrawals += 1;
      snapshot.messages[0].participant_status = "withdrawn";
      snapshot.messages[0].status = "cancelled";
      snapshot.messages[0].session_status = "interrupted";
      snapshot.sessions[0].participation_status = "withdrawn";
      snapshot.sessions[0].status = "interrupted";
      snapshot.pending = 0;
      await route.fulfill({ json: { mode: "dev_custodial" } });
    },
  );
  await page.goto("/messages");
  await page.getByRole("button", { name: "Withdraw bid", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Keep participating" }).click();
  expect(withdrawals).toBe(0);
  await page.getByRole("button", { name: "Withdraw bid", exact: true }).click();
  await dialog
    .getByRole("button", { name: "Withdraw bid", exact: true })
    .click();
  await expect(dialog).toHaveCount(0);
  await page.locator(".message-archive > summary").click();
  await expect(
    page.getByText("Bid withdrawn", { exact: true }).first(),
  ).toBeVisible();
  expect(withdrawals).toBe(1);
});

test("server-confirmed expiry removes approval", async ({ page }) => {
  const snapshot = await fixture(page);
  snapshot.messages[0].participant_status = "disqualified";
  snapshot.messages[0].status = "cancelled";
  snapshot.messages[0].session_status = "interrupted";
  snapshot.messages[0].remaining_ms = 0;
  snapshot.messages[0].timer_running = false;
  snapshot.sessions[0].participation_status = "disqualified";
  snapshot.sessions[0].status = "interrupted";
  snapshot.pending = 0;
  await page.goto("/messages");
  await page.locator(".message-archive > summary").click();
  await expect(
    page.getByText("Disqualified — time budget expired"),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Approve", exact: true }),
  ).toHaveCount(0);
});

test("a failed refresh marks the timer as unconfirmed", async ({ page }) => {
  await fixture(page);
  await page.goto("/messages");
  await expect(page.getByText("10:00")).toBeVisible();

  await page.route("**/api/v1/messages", (route) =>
    route.fulfill({
      status: 503,
      json: { error: "unavailable", message: "Temporarily unavailable" },
    }),
  );
  await page.evaluate(() =>
    window.dispatchEvent(new Event("test-messages-hint")),
  );
  await expect(page.getByText("Reconnecting…")).toBeVisible();
  await expect(
    page.getByText(
      "Connection lost. Remaining time will be confirmed when you reconnect.",
    ),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Approve", exact: true }),
  ).toBeDisabled();
});

test("early approvals keep the timer paused until bidding closes", async ({
  page,
}) => {
  const snapshot = await fixture(page);
  Object.assign(snapshot.messages[0], {
    auction_id: "00000000-0000-4000-8000-000000000050",
    step: 1,
    required_from_ms: Date.now() + 60_000,
    closes_at_ms: Date.now() + 60_000,
    timer_running: false,
  });
  await page.goto("/messages");
  await expect(page.getByText("Preparation", { exact: true })).toBeVisible();
  await expect(page.getByText("10:00")).toBeVisible();
  await expect(page.getByText("Shared auction budget")).toBeVisible();
  await expect(
    page.getByText(/Your timer starts only after bidding closes/),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Approve", exact: true }),
  ).toBeEnabled();

  snapshot.messages[0].status = "responded";
  snapshot.pending = 0;
  Object.assign(snapshot.sessions[0], {
    waiting_until_ms: Date.now() + 7_200_000,
  });
  await page.evaluate(() =>
    window.dispatchEvent(new Event("test-messages-hint")),
  );
  await expect(
    page.getByText(/^Waiting for bidding to close at/),
  ).toBeVisible();
});

test("new activity appears first across request types in compact cards", async ({
  page,
}, info) => {
  const snapshot = await fixture(page);
  snapshot.messages[0].revision = 30;
  snapshot.messages[0].session_title = "Newest private request";
  snapshot.sales = [
    {
      revision: 10,
      created_at_ms: 10,
      auction_id: "sale-auction",
      request_id: "sale-request",
      generation: 1,
      title: "Earlier sale",
      price: "1.25000000",
      buyer: { address: account, name: "Buyer", username: "buyer" },
      remaining_ms: 600000,
      timer_running: false,
      payment_status: "confirmed",
    },
  ];
  snapshot.transactions = [
    {
      id: "payment-request",
      account_id: account,
      auction_id: "payment-auction",
      asset_id: null,
      kind: "payment",
      status: "ready",
      network: "zcash",
      title: "Pay the auction invoice",
      direction: "outgoing",
      counterparty: null,
      auction_title: null,
      created_at_ms: 20,
      updated_at_ms: 20,
      transaction_id: null,
      ethereum_transaction: null,
      nft: null,
      amount: "1.50000000",
      fee: "0.00010000",
      recipient: "local-test-recipient",
      error: null,
      revision: 20,
      can_approve: true,
      can_retry: false,
      remaining_ms: 600000,
      timer_running: false,
      confirmation: null,
    },
  ];
  snapshot.pending = 3;

  await page.goto("/messages");
  const feed = page.getByRole("region", { name: "Awaiting your approval" });
  const titles = feed.locator("article h2, article h3");
  await expect(titles).toHaveText([
    "Contribute your privacy mask",
    "Pay the auction invoice",
    "Approve the sale transfer",
  ]);

  const first = feed.locator("article").first();
  const bounds = await first.boundingBox();
  expect(bounds).not.toBeNull();
  expect(bounds!.height).toBeLessThan(
    info.project.name === "desktop" ? 240 : 370,
  );
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: info.outputPath("compact-inbox.png"),
    fullPage: true,
  });

  snapshot.transactions![0].revision = 40;
  await page.evaluate(() =>
    window.dispatchEvent(new Event("test-messages-hint")),
  );
  await expect(titles).toHaveText([
    "Contribute your privacy mask",
    "Pay the auction invoice",
    "Approve the sale transfer",
  ]);
  await expect(
    feed.getByRole("button", { name: "Approve", exact: true }),
  ).toBeEnabled();

  await page.getByRole("button", { name: "Switch to light theme" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await expect(titles.first()).toHaveText("Contribute your privacy mask");
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: info.outputPath("compact-inbox-light.png"),
    fullPage: true,
  });
});
