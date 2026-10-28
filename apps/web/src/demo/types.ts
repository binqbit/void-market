import type { Account } from "../api/types";

export interface NativeTransaction {
  id: string;
  account_id: string;
  auction_id: string | null;
  asset_id: string | null;
  kind:
    | "mint"
    | "airdrop"
    | "deposit"
    | "return"
    | "delivery"
    | "transfer"
    | "withdrawal"
    | "nft_deposit"
    | "payment";
  status: string;
  network: "ethereum" | "zcash";
  title: string;
  direction: "incoming" | "outgoing";
  counterparty: Account | null;
  auction_title: string | null;
  created_at_ms: number;
  updated_at_ms: number;
  transaction_id: string | null;
  ethereum_transaction: {
    from: string;
    to?: string;
    gas: string;
    gasPrice: string;
    nonce: string;
    chainId: string;
    value: string;
    data: string;
  } | null;
  nft: {
    collection: string;
    token_id: string;
    from: string;
    to: string;
  } | null;
  amount: string | null;
  fee: string | null;
  recipient: string | null;
  error: string | null;
  revision: number;
  can_approve: boolean;
  can_retry: boolean;
  can_cancel?: boolean;
  remaining_ms: number | null;
  timer_running: boolean;
  confirmation: {
    block_hash: string;
    block_number?: string;
    height?: number;
    confirmations?: number;
  } | null;
}

export interface DemoStatus {
  mode: "localnet";
  actors: { account_id: string; alias: string; name: string }[];
  own: null | {
    alias: string;
    ethereum_address: string;
    zcash_address: string;
    balance: string | null;
    wallet_ready: boolean;
  };
  collection: string;
  ethereum_chain_id: number;
  zcash_network: string;
  claim_amount: string;
}

export function shortAddress(value: string) {
  return value.length > 30
    ? `${value.slice(0, 12)}…${value.slice(-10)}`
    : value;
}
