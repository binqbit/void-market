import { ArrowUpRight, Clock3, LockKeyhole } from "lucide-react";
import { Link } from "react-router-dom";
import { AssetImage } from "../components/asset-image";
import { accountName, auctionReasons } from "./types";
import { isAuctionFinished } from "./status";
import { AuctionStatusBadge } from "./status-badge";
import type { Auction } from "./types";

export function AuctionCard({ auction }: { auction: Auction }) {
  const finished = isAuctionFinished(auction.status);

  return (
    <article className="asset-card auction-card">
      <Link
        className="auction-image-link"
        to={`/market/${auction.id}`}
        aria-label={`View ${auction.asset.name}`}
      >
        <AssetImage image={auction.asset.image} name={auction.asset.name} />
      </Link>
      <div className="asset-card-body">
        <AuctionStatusBadge auction={auction} />
        <h2>
          <Link to={`/market/${auction.id}`}>{auction.asset.name}</Link>
        </h2>
        <p>{auction.asset.description || "No description provided."}</p>
        {auction.reason && auctionReasons[auction.reason] && (
          <p>{auctionReasons[auction.reason]}</p>
        )}
        <div className="auction-card-meta">
          <LockKeyhole size={15} />
          Private bids · ZEC
        </div>
        <div className="auction-card-meta">
          <Clock3 size={15} />
          {finished && auction.finished_at_ms !== null
            ? `Finished ${new Date(auction.finished_at_ms).toLocaleString()}`
            : auction.status === "draft"
              ? "Not published"
              : `${finished ? "Bidding closed" : "Bids close"} ${new Date(auction.bidding_closes_at_ms ?? auction.closes_at_ms - 3600000).toLocaleString()}`}
        </div>
        <div className="auction-card-foot">
          <span>{accountName(auction.seller)}</span>
          <Link className="text-link" to={`/market/${auction.id}`}>
            View auction <ArrowUpRight size={16} />
          </Link>
        </div>
      </div>
    </article>
  );
}
