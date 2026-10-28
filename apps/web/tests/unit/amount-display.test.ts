import { describe, expect, it } from "vitest";
import { formatZec, normalizeZecInput, zecInputError } from "../../src/amounts";

describe("ZEC display", () => {
  it.each([
    ["1.00000000", "1.0"],
    ["1.25000000", "1.25"],
    ["0", "0.0"],
    ["0.00000001", "0.00000001"],
    ["90071992.54740993", "90071992.54740993"],
    ["92233720368.54775807", "92233720368.54775807"],
  ])("preserves exact value of %s", (input, expected) => {
    expect(formatZec(input)).toBe(expected);
  });

  it("accepts a decimal comma without treating it as a thousands separator", () => {
    expect(normalizeZecInput("1,25")).toBe("1.25");
    expect(zecInputError(normalizeZecInput("1,25"))).toBeNull();
    expect(zecInputError(normalizeZecInput("1,000,000"))).not.toBeNull();
  });

  it("explains invalid precision, zero bids and amounts above the supported range", () => {
    expect(zecInputError("0")).toContain("greater than zero");
    expect(zecInputError("0", true)).toBeNull();
    expect(zecInputError("0.000000001")).toContain("8 decimal");
    expect(zecInputError("92233720368.54775808")).toContain("too large");
  });
});
