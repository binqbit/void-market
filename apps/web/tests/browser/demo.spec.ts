import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";
import type { NativeTransaction } from "../../src/demo/types";

const account = "00000000-0000-4000-8000-000000000001";
const transaction: NativeTransaction = {
  id: "00000000-0000-4000-8000-000000000010",
  account_id: account,
  auction_id: "00000000-0000-4000-8000-000000000020",
  asset_id: null,
  kind: "payment",
  status: "ready",
  network: "zcash",
  title: "Pay the auction invoice",
  transaction_id: null,
  ethereum_transaction: null,
  nft: null,
  amount: "1.50000000",
  fee: "0.00010000",
  recipient: `uregtest1${"a".repeat(170)}`,
  error: null,
  revision: 7,
  can_approve: true,
  remaining_ms: 580000,
  timer_running: true,
  confirmation: null,
};

async function fixture(page: Page, enabled = true) {
  await page.addInitScript(() => {
    class Events extends EventTarget {
      close() {}
    }
    Object.defineProperty(window, "EventSource", { value: Events });
  });
  await page.route("**/api/v1/auth/config", (route) =>
    route.fulfill({ json: { google: true, demo: enabled } }),
  );
  await page.route("**/api/v1/me", (route) =>
    route.fulfill({
      json: { address: account, name: "Bob", username: "demo_bob" },
    }),
  );
  await page.route("**/api/v1/demo", (route) =>
    route.fulfill({
      json: {
        mode: "localnet",
        own: { alias: "bob", balance: "10.00000000", wallet_ready: true },
        actors: [],
        collection: "0x1234",
        ethereum_chain_id: 31337,
      },
    }),
  );
  await page.route("**/api/v1/me/assets?*", (route) =>
    route.fulfill({ json: [] }),
  );
  const snapshot = {
    custody_mode: "development",
    revision: 7,
    server_time_ms: Date.now(),
    pending: 1,
    messages: [],
    sessions: [],
    sales: [],
    transactions: [{ ...transaction }],
  };
  await page.route("**/api/v1/messages", (route) =>
    route.fulfill({ json: snapshot }),
  );
  return snapshot;
}

test("demo remains usable while an auction payment needs approval", async ({
  page,
}, info) => {
  const snapshot = await fixture(page);
  const requests: unknown[] = [];
  await page.route(
    `**/api/v1/transactions/${transaction.id}/approve`,
    async (route) => {
      requests.push(route.request().postDataJSON());
      snapshot.transactions[0].status = "submitted";
      snapshot.transactions[0].can_approve = false;
      snapshot.transactions[0].revision = 8;
      snapshot.pending = 0;
      await route.fulfill({ json: snapshot.transactions[0], status: 202 });
    },
  );
  await page.goto("/demo");
  await expect(
    page.getByRole("heading", { name: "Try the whole exchange." }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Claim 10 ZEC" }),
  ).toBeEnabled();
  await expect(
    page.getByRole("button", { name: "Continue as Bob" }),
  ).toBeDisabled();
  await expect(
    page.getByRole("button", { name: "Continue as Alice" }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "Review & approve" }).click();
  const dialog = page.getByRole("dialog");
  await expect(
    dialog.getByText("0.00010000 ZEC", { exact: true }),
  ).toBeVisible();
  await expect(dialog.getByText(transaction.recipient!)).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: info.outputPath("local-payment-review.png"),
    fullPage: true,
  });
  await dialog.getByRole("button", { name: "Approve & send" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByText("Waiting for confirmations…")).toBeVisible();
  expect(requests).toEqual([{ revision: 7 }]);
});

test("normal mode hides the demo entry and cannot open the demo page", async ({
  page,
}) => {
  await fixture(page, false);
  await page.goto("/demo");
  await expect(page).toHaveURL(/\/$/);
  await expect(
    page.getByRole("link", { name: "Demo", exact: true }),
  ).toHaveCount(0);
});
