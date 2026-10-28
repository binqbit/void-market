import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";
import type { Asset } from "../../src/api/types";
import type { NativeTransaction } from "../../src/demo/types";

const account = "00000000-0000-4000-8000-000000000001";
const assetId = "00000000-0000-4000-8000-000000000002";
const operation = "00000000-0000-4000-8000-000000000003";
const destination = "0x3333333333333333333333333333333333333333";
const collection = "0x2222222222222222222222222222222222222222";
const wallet = "0x1111111111111111111111111111111111111111";
const nft: Asset = {
  id: assetId,
  owner_id: account,
  name: "Dark Matter",
  description: "A digital collectible.",
  image: null,
  created_at: new Date().toISOString(),
  active_auction_id: null,
  pending_transaction_id: null,
  nft: { chain_id: 31337, collection, token_id: "42", genesis: "genesis" },
};

async function fixture(page: Page, assets: Asset[]) {
  const state = { assets, transactions: [] as NativeTransaction[] };
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
    route.fulfill({
      json: { address: account, name: "Alice", username: "alice" },
    }),
  );
  await page.route("**/api/v1/me/assets?*", (route) =>
    route.fulfill({ json: state.assets }),
  );
  await page.route("**/api/v1/me/nft-deposit", (route) =>
    route.fulfill({ json: { address: wallet, chain_id: 31337 } }),
  );
  await page.route("**/api/v1/me/transactions?*", (route) =>
    route.fulfill({ json: state.transactions }),
  );
  await page.route("**/api/v1/messages", (route) =>
    route.fulfill({
      json: {
        custody_mode: "development",
        revision: 1,
        server_time_ms: Date.now(),
        pending: state.transactions.filter((item) => item.can_approve).length,
        messages: [],
        sessions: [],
        sales: [],
        transactions: state.transactions,
      },
    }),
  );
  return state;
}

test("NFT withdrawal accepts an external address and keeps the NFT until confirmation", async ({
  page,
}) => {
  const state = await fixture(page, [{ ...nft }]);
  const submitted: { recipient_address: string; request_id: string }[] = [];
  await page.route(`**/api/v1/assets/${assetId}/withdraw`, async (route) => {
    submitted.push(route.request().postDataJSON());
    state.assets[0].pending_transaction_id = operation;
    const transfer: NativeTransaction = {
      id: operation,
      account_id: account,
      auction_id: null,
      asset_id: assetId,
      kind: "withdrawal",
      status: "ready",
      network: "ethereum",
      title: "Withdraw NFT",
      direction: "outgoing",
      counterparty: null,
      auction_title: null,
      created_at_ms: Date.now(),
      updated_at_ms: Date.now(),
      transaction_id: null,
      ethereum_transaction: {
        from: wallet,
        to: collection,
        gas: "0x10000",
        gasPrice: "0x1",
        nonce: "0x0",
        chainId: "0x7a69",
        value: "0x0",
        data: "0x",
      },
      nft: { collection, token_id: "42", from: wallet, to: destination },
      amount: null,
      fee: null,
      recipient: null,
      error: null,
      revision: 1,
      can_approve: true,
      can_retry: false,
      remaining_ms: null,
      timer_running: false,
      confirmation: null,
    };
    state.transactions = [transfer];
    await route.fulfill({ status: 202, json: transfer });
  });
  await page.goto("/portfolio");
  await expect(
    page.getByRole("button", { name: "Transfer", exact: true }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: "Withdraw", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("combobox")).toHaveCount(0);
  const review = dialog.getByRole("button", { name: "Review withdrawal" });
  await expect(review).toBeDisabled();
  await dialog.getByLabel("Destination Ethereum address").fill(destination);
  await review.click();
  await expect(page).toHaveURL(/\/messages$/);
  expect(submitted[0].recipient_address).toBe(destination);
  await page.getByRole("button", { name: "Review & approve" }).click();
  await expect(page.getByRole("dialog")).toContainText(destination);
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Back", exact: true })
    .click();
  await page.goto("/portfolio");
  await expect(
    page.getByRole("heading", { name: "Dark Matter" }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Withdraw", exact: true }),
  ).toBeDisabled();
  state.assets = [];
  state.transactions[0].status = "confirmed";
  state.transactions[0].can_approve = false;
  await page.reload();
  await expect(page.getByRole("heading", { name: "Dark Matter" })).toHaveCount(
    0,
  );
});

test("deposit shows a personal address and verifies the entered NFT transaction", async ({
  page,
}) => {
  const state = await fixture(page, []);
  const attempts: unknown[] = [];
  await page.route("**/api/v1/nft-deposits", async (route) => {
    attempts.push(route.request().postDataJSON());
    if (attempts.length === 1) {
      await route.fulfill({
        status: 409,
        json: { message: "The deposit has not been confirmed yet" },
      });
    } else {
      state.assets = [{ ...nft }];
      await route.fulfill({ json: nft });
    }
  });
  await page.goto("/portfolio");
  await page.getByRole("button", { name: "Deposit NFT" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText(wallet);
  await expect(dialog).toContainText("31337");
  await dialog.getByLabel("Transaction hash").fill(`0x${"a".repeat(64)}`);
  await dialog.getByLabel("NFT contract address").fill(collection);
  await dialog.getByLabel("Token ID", { exact: true }).fill("42");
  await dialog.getByRole("button", { name: "Confirm deposit" }).click();
  await expect(dialog.getByRole("alert")).toContainText("not been confirmed");
  await dialog.getByRole("button", { name: "Confirm deposit" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(
    page.getByRole("heading", { name: "Dark Matter" }),
  ).toBeVisible();
  expect(attempts).toHaveLength(2);
  expect(attempts[0]).toEqual(attempts[1]);
});
