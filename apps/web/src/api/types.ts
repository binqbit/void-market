export interface Account {
  address: string;
  name: string;
  username: string | null;
}

export interface Profile extends Account {
  email: string | null;
}

export interface Asset {
  nft?: {
    chain_id: number;
    collection: string;
    token_id: string;
    genesis: string;
  } | null;
  pending_transaction_id?: string | null;
  id: string;
  name: string;
  description: string;
  image: string | null;
  owner_id: string;
  active_auction_id?: string | null;
  created_at: string;
}

export interface Transfer {
  id: string;
  asset_id: string;
  sender_id: string;
  recipient_id: string;
  auction_id?: string | null;
  created_at: string;
}
