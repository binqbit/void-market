import { expect, test } from "vitest";
import { safeReturnTo } from "../../src/auth/return-to";

test("sign-in return paths remain inside supported application routes", () => {
  expect(safeReturnTo("/market/123?view=bid#rules")).toBe(
    "/market/123?view=bid#rules",
  );
  for (const path of [
    null,
    "https://example.org",
    "//example.org",
    "/\\example.org",
    "/api/v1/auth/logout",
  ]) {
    expect(safeReturnTo(path)).toBe("/portfolio");
  }
});
