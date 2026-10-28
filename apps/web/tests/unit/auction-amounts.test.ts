import { describe, expect, it } from "vitest";
import { validAmount } from "../../src/auctions/types";

describe("ZEC price entry", () => {
  it("keeps eight decimals and large amounts exact", () => {
    for (const value of [
      "0.00000001",
      "1.25",
      "90071992.54740993",
      "92233720368.54775807",
    ])
      expect(validAmount(value)).toBe(true);
    for (const value of [
      "0",
      "1e3",
      "1.000000001",
      "92233720368.54775808",
      "-1",
      "1,25",
    ])
      expect(validAmount(value)).toBe(false);
    expect(validAmount("0", true)).toBe(true);
  });
});
