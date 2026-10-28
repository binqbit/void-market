import { ArrowUpRight, ChevronDown } from "lucide-react";
import { useId, useState } from "react";
import { Link } from "react-router-dom";
import { useAuctionQuery } from "../auctions/use-auctions";
import { ErrorNotice, Loading } from "../components/feedback";
import { BidRow } from "./bid-row";
import type { BidActivity } from "./types";

export function BiddingOverview() {
  const [expanded, setExpanded] = useState(false);
  const [page, setPage] = useState(0);
  const id = useId();
  const activity = useAuctionQuery<BidActivity>(
    `/v1/me/bids?limit=10&offset=${page * 10}`,
  );
  const data = activity.data;

  return (
    <section className="panel bidding-overview" aria-labelledby={`${id}-title`}>
      <div className="bidding-heading">
        <div>
          <h2 id={`${id}-title`}>Your bidding</h2>
          <p>Track your bids and the approvals that need you.</p>
        </div>
        <button
          className="button small"
          aria-expanded={expanded}
          aria-controls={`${id}-bids`}
          disabled={!data}
          onClick={() => setExpanded(!expanded)}
        >
          My bids <ChevronDown size={16} aria-hidden="true" />
        </button>
      </div>

      {activity.error && (
        <ErrorNotice retry={activity.refresh}>
          {data
            ? "Could not refresh your bidding activity. Showing the last update."
            : activity.error}
        </ErrorNotice>
      )}
      {activity.loading && <Loading>Loading your bidding activity…</Loading>}
      {data && (
        <>
          <dl className="bidding-stats">
            <div>
              <dt>Bids placed</dt>
              <dd>{data.stats.bids_placed}</dd>
            </div>
            <div>
              <dt>Active bids</dt>
              <dd>{data.stats.active_bids}</dd>
            </div>
            <div>
              <dt>Completed purchases</dt>
              <dd>{data.stats.completed_purchases}</dd>
            </div>
            <div
              className={
                data.stats.pending_approvals > 0 ? "approvals-due" : undefined
              }
            >
              <dt>Approvals to review</dt>
              <dd>
                {data.stats.pending_approvals > 0 && !activity.error ? (
                  <Link
                    to="/messages"
                    aria-label={`Review ${data.stats.pending_approvals} pending approvals`}
                  >
                    {data.stats.pending_approvals}{" "}
                    <ArrowUpRight size={18} aria-hidden="true" />
                  </Link>
                ) : (
                  data.stats.pending_approvals
                )}
              </dd>
            </div>
          </dl>
          <div id={`${id}-bids`} className="account-bids" hidden={!expanded}>
            {data.bids.length ? (
              <>
                <p className="field-help">
                  Progress covers received requests. More may arrive while an
                  auction is active.
                </p>
                <ul className="account-bid-list">
                  {data.bids.map((bid) => (
                    <BidRow
                      key={bid.auction_id}
                      bid={bid}
                      now={data.server_time_ms}
                    />
                  ))}
                </ul>
              </>
            ) : (
              <div className="bidding-empty">
                <p>You haven’t placed any bids yet.</p>
                <Link className="text-link" to="/market">
                  Explore the market{" "}
                  <ArrowUpRight size={16} aria-hidden="true" />
                </Link>
              </div>
            )}
            {(page > 0 || data.has_more) && (
              <nav className="pagination" aria-label="Bid history pages">
                <button
                  className="button small"
                  disabled={page === 0}
                  onClick={() => setPage(page - 1)}
                >
                  Previous
                </button>
                <span>Page {page + 1}</span>
                <button
                  className="button small"
                  disabled={!data.has_more}
                  onClick={() => setPage(page + 1)}
                >
                  Next
                </button>
              </nav>
            )}
          </div>
        </>
      )}
    </section>
  );
}
