import { ArrowUpRight } from "lucide-react";
import { Link } from "react-router-dom";
import { formatZec } from "../amounts";
import { statusLabels } from "../auctions/types";
import type { AccountBid } from "./types";

function status(bid: AccountBid, now: number): string {
  if (bid.participation_status === "withdrawn") return "Withdrawn";
  if (bid.participation_status === "disqualified") return "Disqualified";
  if (bid.auction_status === "completed")
    return bid.is_winner ? "Purchased" : "Not selected";
  if (bid.auction_status === "awaiting_completion")
    return bid.is_winner ? "Selected · completing sale" : "Not selected";
  if (bid.auction_status === "open")
    return now >= bid.bidding_closes_at_ms
      ? "Approvals in progress"
      : "Bid placed";
  return statusLabels[bid.auction_status];
}

export function BidRow({ bid, now }: { bid: AccountBid; now: number }) {
  const { approved, requested, pending } = bid.approvals;
  const active =
    bid.participation_status === "active" &&
    (["open", "matching"].includes(bid.auction_status) ||
      (bid.auction_status === "awaiting_completion" && bid.is_winner));

  return (
    <li className="account-bid">
      <div className="account-bid-description">
        <Link className="account-bid-title" to={`/market/${bid.auction_id}`}>
          {bid.asset_name} <ArrowUpRight size={15} aria-hidden="true" />
        </Link>
        <span className="account-bid-status">{status(bid, now)}</span>
        {active && (
          <time dateTime={new Date(bid.closes_at_ms).toISOString()}>
            Deadline {new Date(bid.closes_at_ms).toLocaleString()}
          </time>
        )}
      </div>
      <div className="account-bid-amount">
        <span>Your private bid</span>
        <strong>{formatZec(bid.amount)} ZEC</strong>
      </div>
      <div className="account-bid-progress">
        {requested > 0 ? (
          <>
            <span>
              {approved} of {requested} requests approved
            </span>
            <progress
              max={requested}
              value={approved}
              aria-label={`Approvals for ${bid.asset_name}`}
            />
          </>
        ) : (
          <span>
            {active ? "No approval requests yet" : "No approval requests"}
          </span>
        )}
        {pending > 0 && (
          <Link className="text-link" to="/messages">
            Review {pending} {pending === 1 ? "approval" : "approvals"}
            <ArrowUpRight size={14} aria-hidden="true" />
          </Link>
        )}
      </div>
    </li>
  );
}
