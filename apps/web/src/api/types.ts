export interface Account {
  address: string;
  name: string;
  username: string | null;
}

export interface Profile extends Account {
  email: string | null;
  is_demo?: boolean;
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
  owner_id: string | null;
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
