import { ShieldCheck, Store } from "lucide-react";
import { useState } from "react";
import { Link } from "react-router-dom";
import { AuctionCard } from "../auctions/card";
import { useAuctionQuery } from "../auctions/use-auctions";
import type { Auction } from "../auctions/types";
import { ErrorNotice, Loading } from "../components/feedback";
import { useDemoMode } from "../demo/config";

export function MarketPage() {
  const demo = useDemoMode();
  const [page, setPage] = useState(0);
  const query = useAuctionQuery<Auction[]>(
    `/v1/auctions?limit=20&offset=${page * 20}`,
  );
  return (
    <div className="page-width page-content">
      <div className="page-heading">
        <div>
          <span className="eyebrow">DISCOVER THE NEXT CHAPTER</span>
          <h1>The market</h1>
          <p>Assets in view. Bids under wraps.</p>
        </div>
        <Link className="button" to="/portfolio">
          Sell an asset
        </Link>
      </div>
      <div className="custody-note">
        <ShieldCheck size={17} />
        <span>
          {demo
            ? "Demo localnet · Prices in ZEC. NFT transfers and payments run on local test networks with no real monetary value."
            : "Development custody · Private bid comparisons. Prices in ZEC; payment is not processed or verified."}
        </span>
      </div>
      {query.error && (
        <ErrorNotice retry={query.refresh}>{query.error}</ErrorNotice>
      )}
      {query.loading ? (
        <Loading>Loading auctions…</Loading>
      ) : query.data?.length === 0 ? (
        <section className="empty-state market-empty">
          <span className="empty-icon">
            <Store size={30} />
          </span>
          <h2>No auctions yet.</h2>
          <p>Choose an asset in your portfolio to publish the first auction.</p>
          <Link className="button primary" to="/portfolio">
            Open my portfolio
          </Link>
        </section>
      ) : (
        <div className="asset-grid">
          {query.data?.map((auction) => (
            <AuctionCard key={auction.id} auction={auction} />
          ))}
        </div>
      )}
      {(page > 0 || query.data?.length === 20) && (
        <div className="pagination">
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
            disabled={query.data?.length !== 20}
            onClick={() => setPage(page + 1)}
          >
            Next
          </button>
        </div>
      )}
    </div>
  );
}
