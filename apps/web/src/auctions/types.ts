import type { Account } from "../api/types";

export type AuctionStatus =
  | "draft"
  | "deposit_pending"
  | "open"
  | "matching"
  | "awaiting_completion"
  | "completed"
  | "no_sale"
  | "interrupted"
  | "cancelled";

export interface Auction {
  id: string;
  asset: {
    id: string;
    name: string;
    description: string;
    image: string | null;
  };
  seller: Account;
  winner: Account | null;
  status: AuctionStatus;
  reason: string | null;
  currency: "ZEC";
  execution: "development_custody" | "localnet_custody";
  payment_status: "unverified" | "pending" | "confirmed" | "review";
  custody_status?: string;
  settlement_note?: string | null;
  payment?: { expected: string; paid: string; remaining: string } | null;
  wallet_ready: boolean;
  closes_at_ms: number;
  required_from_ms?: number;
  created_at_ms: number;
  published_at_ms: number | null;
  finished_at_ms: number | null;
  revision: number;
  policy_hash: string | null;
  buyer_limit: number;
  admitted_buyers: number;
  active_buyers: number;
  generation: number;
  checks_completed: number;
  checks_total: number;
  reserve: string | null;
  your_bid: string | null;
  agreed_price: string | null;
  participation: {
    status: "active" | "withdrawn" | "disqualified";
    remaining_ms: number;
    timer_running: boolean;
    created_at_ms: number;
  } | null;
  completion_request_id: string | null;
  actions: {
    edit: boolean;
    publish: boolean;
    cancel: boolean;
    bid: boolean;
    withdraw: boolean;
    complete: boolean;
  };
}

export interface SaleRequest {
  revision: number;
  payment_status?: string;
  auction_id: string;
  request_id: string;
  generation: number;
  title: string;
  price: string;
  buyer: Account;
  remaining_ms: number;
  timer_running: boolean;
}

export const statusLabels: Record<AuctionStatus, string> = {
  draft: "Draft",
  deposit_pending: "Awaiting NFT deposit",
  open: "Open for bids",
  matching: "Checking bids",
  awaiting_completion: "Awaiting seller",
  completed: "Sale completed",
  no_sale: "No sale",
  interrupted: "Interrupted",
  cancelled: "Draft cancelled",
};

export function accountName(account: Account): string {
  return account.username ? `@${account.username}` : account.name;
}

export function localDateTime(milliseconds: number): string {
  const date = new Date(milliseconds);
  return new Date(milliseconds - date.getTimezoneOffset() * 60000)
    .toISOString()
    .slice(0, 16);
}

export function validAmount(value: string, allowZero = false): boolean {
  if (value.length > 32) return false;
  if (!/^\d+(?:\.\d{1,8})?$/.test(value)) return false;
  const [whole, decimals = ""] = value.split(".");
  const units = BigInt(whole) * 100000000n + BigInt(decimals.padEnd(8, "0"));
  return units <= 9223372036854775807n && (allowZero || units > 0n);
}
