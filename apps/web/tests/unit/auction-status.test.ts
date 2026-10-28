import { describe, expect, it } from "vitest";
import { isAuctionFinished, auctionStatus } from "../../src/auctions/status";
import type { AuctionStatus } from "../../src/auctions/types";

describe("auction display status", () => {
  it.each<AuctionStatus>(["open", "matching", "awaiting_completion"])(
    "marks %s as active",
    (status) => {
      expect(auctionStatus({ status }).label).toBe("Active");
      expect(auctionStatus({ status }).tone).toBe("active");
      expect(isAuctionFinished(status)).toBe(false);
    },
  );

  it("separates a completed sale from a confirmed NFT return", () => {
    expect(
      auctionStatus({ status: "completed", custody_status: "delivered" }).label,
    ).toBe("Sold");
    expect(
      auctionStatus({ status: "no_sale", custody_status: "returned" }).label,
    ).toBe("Refunded");
    expect(
      auctionStatus({ status: "interrupted", custody_status: "returned" })
        .label,
    ).toBe("Refunded");
  });

  it("does not claim an NFT was returned before confirmation", () => {
    expect(
      auctionStatus({ status: "no_sale", custody_status: "return_pending" })
        .label,
    ).toBe("Return pending");
    expect(
      auctionStatus({ status: "no_sale", custody_status: "held" }).label,
    ).toBe("No sale");
    expect(
      auctionStatus({ status: "interrupted", custody_status: "review" }).label,
    ).toBe("Needs review");
  });

  it("keeps pending transfers blue and separates completed outcomes", () => {
    expect(auctionStatus({ status: "deposit_pending" }).tone).toBe("active");
    expect(
      auctionStatus({ status: "no_sale", custody_status: "return_pending" })
        .tone,
    ).toBe("active");
    expect(
      auctionStatus({ status: "interrupted", custody_status: "review" }).tone,
    ).toBe("active");
    expect(auctionStatus({ status: "completed" }).tone).toBe("successful");
    expect(
      auctionStatus({ status: "no_sale", custody_status: "returned" }).tone,
    ).toBe("unsuccessful");
    expect(auctionStatus({ status: "cancelled" }).tone).toBe("unsuccessful");
  });

  it.each<AuctionStatus>(["completed", "no_sale", "interrupted", "cancelled"])(
    "recognizes %s as finished even before its original deadline",
    (status) => expect(isAuctionFinished(status)).toBe(true),
  );
});
