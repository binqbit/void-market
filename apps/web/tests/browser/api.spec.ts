import { readFileSync } from "node:fs";
import { expect, test } from "@playwright/test";

interface Identity {
  address: string;
  token: string;
}
interface Fixture {
  owner: Identity;
  recipient: Identity;
}

test("real API: image upload, asset ownership, transfer receipt and sign-out", async ({
  page,
  browser,
}, testInfo) => {
  test.skip(
    !process.env.WEB_API_FIXTURE_FILE || testInfo.project.name !== "desktop",
    "Requires isolated API identities; never use production accounts",
  );
  const fixture = JSON.parse(
    readFileSync(process.env.WEB_API_FIXTURE_FILE!, "utf8"),
  ) as Fixture;
  await page.context().addCookies([
    {
      name: "market_session",
      value: fixture.owner.token,
      domain: "localhost",
      path: "/",
      httpOnly: true,
      sameSite: "Lax",
    },
  ]);
  await page.goto("/portfolio");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(
    "My portfolio",
  );
  await page.getByRole("button", { name: "Add an asset", exact: true }).click();
  const name = `Browser asset ${Date.now()}`;
  await page.getByLabel("Asset name").fill(name);
  await page
    .getByLabel("Description", { exact: false })
    .fill("Created by the real browser/API integration check.");
  await page.locator('input[type="file"]').setInputFiles({
    name: "asset.png",
    mimeType: "image/png",
    buffer: Buffer.from(
      "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a7X8AAAAASUVORK5CYII=",
      "base64",
    ),
  });
  await page
    .getByRole("button", { name: "Add to portfolio", exact: true })
    .click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  const card = page
    .getByRole("article")
    .filter({ has: page.getByRole("heading", { name, exact: true }) });
  await expect(card).toBeVisible();
  await expect(card.getByRole("img")).toBeVisible();
  expect(
    await card
      .getByRole("img")
      .evaluate((image) => (image as HTMLImageElement).naturalWidth),
  ).toBeGreaterThan(0);
  await card.getByRole("button", { name: "Transfer", exact: true }).click();
  await page
    .getByLabel("Recipient account address")
    .fill(fixture.recipient.address);
  await page.getByRole("button", { name: "Find recipient" }).click();
  await page.getByRole("button", { name: "Confirm transfer" }).click();
  await expect(
    page.getByRole("heading", { name: "Transfer complete" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await expect(page.getByRole("heading", { name, exact: true })).toHaveCount(0);
  await page.getByRole("tab", { name: "History", exact: true }).click();
  await expect(
    page.getByText("Asset sent", { exact: true }).first(),
  ).toBeVisible();

  const other = await browser.newContext();
  try {
    await other.addCookies([
      {
        name: "market_session",
        value: fixture.recipient.token,
        domain: "localhost",
        path: "/",
        httpOnly: true,
        sameSite: "Lax",
      },
    ]);
    const recipient = await other.newPage();
    await recipient.goto("http://localhost:3000/portfolio");
    await expect(
      recipient.getByRole("heading", { name, exact: true }),
    ).toBeVisible();
    await recipient.getByRole("tab", { name: "History", exact: true }).click();
    await expect(
      recipient.getByText("Asset received", { exact: true }).first(),
    ).toBeVisible();
  } finally {
    await other.close();
  }

  await page.goto("/account");
  await page.getByRole("button", { name: "Sign out", exact: true }).click();
  await page.goto("/portfolio");
  await expect(
    page.getByRole("heading", { name: "Your portfolio starts here." }),
  ).toBeVisible();
  expect(
    (await page.context().cookies()).find(
      (cookie) => cookie.name === "market_session",
    ),
  ).toBeUndefined();
});
