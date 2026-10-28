import { auctionStatus } from "./status";
import type { Auction } from "./types";

export function AuctionStatusBadge({
  auction,
}: {
  auction: Pick<Auction, "status" | "custody_status">;
}) {
  const status = auctionStatus(auction);

  return (
    <div className="auction-state">
      <span className={`auction-status status-${status.tone}`}>
        {status.label}
      </span>
      {status.detail && <span className="auction-stage">{status.detail}</span>}
    </div>
  );
}
