import { ArrowUpRight } from "lucide-react";
import { Link } from "react-router-dom";
import { auctionReasons, statusLabels } from "../auctions/types";
import type { MessageSnapshot } from "./types";

export function AuctionOutcomes({
  outcomes,
}: {
  outcomes: NonNullable<MessageSnapshot["outcomes"]>;
}) {
  if (outcomes.length === 0) return null;

  return (
    <section className="message-history" aria-label="Auction results">
      <h2>Auction results</h2>
      <div className="message-list">
        {outcomes.map((outcome) => (
          <article
            className="message-session auction-outcome"
            key={outcome.auction_id}
          >
            <div>
              <span className={`auction-status status-${outcome.status}`}>
                {statusLabels[outcome.status]}
              </span>
              <h3>{outcome.title}</h3>
              {outcome.reason && auctionReasons[outcome.reason] && (
                <p>{auctionReasons[outcome.reason]}</p>
              )}
              <Link className="text-link" to={`/market/${outcome.auction_id}`}>
                View auction <ArrowUpRight size={16} />
              </Link>
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}
