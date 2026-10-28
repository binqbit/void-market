import { statusLabels } from "./types";
import type { Auction, AuctionStatus } from "./types";

type StatusSource = Pick<Auction, "status" | "custody_status">;

interface DisplayStatus {
  label: string;
  tone: "active" | "successful" | "unsuccessful";
  detail: string | null;
}

export function auctionStatus(auction: StatusSource): DisplayStatus {
  if (["open", "matching", "awaiting_completion"].includes(auction.status)) {
    return {
      label: "Active",
      tone: "active",
      detail: statusLabels[auction.status],
    };
  }
  if (auction.status === "completed") {
    return { label: "Sold", tone: "successful", detail: null };
  }
  if (["no_sale", "interrupted"].includes(auction.status)) {
    if (auction.custody_status === "returned") {
      return {
        label: "Refunded",
        tone: "unsuccessful",
        detail: "NFT returned to seller",
      };
    }
    if (auction.custody_status === "return_pending") {
      return { label: "Return pending", tone: "active", detail: null };
    }
    if (auction.custody_status === "review") {
      return { label: "Needs review", tone: "active", detail: null };
    }
  }
  return {
    label: statusLabels[auction.status],
    tone: isAuctionFinished(auction.status) ? "unsuccessful" : "active",
    detail: null,
  };
}

export function isAuctionFinished(status: AuctionStatus): boolean {
  return ["completed", "no_sale", "interrupted", "cancelled"].includes(status);
}
