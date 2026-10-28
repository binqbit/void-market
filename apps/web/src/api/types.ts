export interface Account {
  address: string;
  name: string;
  username: string | null;
}

export interface Profile extends Account {
  email: string | null;
}

export interface Asset {
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
