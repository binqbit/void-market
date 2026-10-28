import { useState } from "react";
import { ErrorNotice, Loading } from "../components/feedback";
import { AuctionCard } from "./card";
import { useAuctionQuery } from "./use-auctions";
import type { Auction } from "./types";

export function MyAuctions() {
  const [page, setPage] = useState(0);
  const auctions = useAuctionQuery<Auction[]>(
    `/v1/me/auctions?limit=20&offset=${page * 20}`,
  );
  return (
    <>
      {auctions.error && (
        <ErrorNotice retry={auctions.refresh}>{auctions.error}</ErrorNotice>
      )}
      {auctions.loading ? (
        <Loading>Loading your auctions…</Loading>
      ) : auctions.data?.length === 0 ? (
        <div className="empty-state panel">
          <h2>No auctions yet.</h2>
          <p>List an asset from your portfolio or place a bid in the market.</p>
        </div>
      ) : (
        <div className="asset-grid">
          {auctions.data?.map((auction) => (
            <AuctionCard key={auction.id} auction={auction} />
          ))}
        </div>
      )}
      {(page > 0 || auctions.data?.length === 20) && (
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
            disabled={auctions.data?.length !== 20}
            onClick={() => setPage(page + 1)}
          >
            Next
          </button>
        </div>
      )}
    </>
  );
}
