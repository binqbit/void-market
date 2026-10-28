import type { AuctionStatus } from "../auctions/types";

export interface BidActivity {
  stats: {
    bids_placed: number;
    active_bids: number;
    completed_purchases: number;
    pending_approvals: number;
  };
  bids: AccountBid[];
  has_more: boolean;
  server_time_ms: number;
}

export interface AccountBid {
  auction_id: string;
  asset_name: string;
  amount: string;
  auction_status: AuctionStatus;
  participation_status: "active" | "withdrawn" | "disqualified";
  is_winner: boolean;
  created_at_ms: number;
  closes_at_ms: number;
  bidding_closes_at_ms: number;
  approvals: {
    requested: number;
    approved: number;
    pending: number;
  };
}
