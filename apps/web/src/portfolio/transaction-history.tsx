import { ArrowDownLeft, ArrowUpRight } from "lucide-react";
import { useState } from "react";
import { Link } from "react-router-dom";
import { formatZec } from "../amounts";
import { useAuctionQuery } from "../auctions/use-auctions";
import { ErrorNotice, Loading } from "../components/feedback";
import { TransactionCard } from "../demo/transaction-card";
import type { NativeTransaction } from "../demo/types";
import { shortAddress } from "../demo/types";

const labels: Record<string, string> = {
  preparing: "Preparing",
  ready: "Needs approval",
  approved: "Approved",
  building: "Preparing transaction",
  signed: "Ready to send",
  submitted: "Awaiting confirmation",
  confirmed: "Confirmed",
  failed: "Failed",
  reconcile: "Checking transaction",
  cancelled: "Cancelled",
};

export function TransactionHistory() {
  const [page, setPage] = useState(0);
  const query = useAuctionQuery<NativeTransaction[]>(
    `/v1/me/transactions?limit=21&offset=${page * 20}`,
  );
  if (query.loading) return <Loading>Loading transaction history…</Loading>;

  return (
    <>
      {query.error && (
        <ErrorNotice retry={query.refresh}>{query.error}</ErrorNotice>
      )}
      {query.data?.length === 0 && (
        <div className="empty-state panel">
          <h2>{page ? "No more transactions" : "No transactions yet"}</h2>
        </div>
      )}
      <div className="history-list">
        {query.data?.slice(0, 20).map((item) => (
          <details className="transaction-history-item" key={item.id}>
            <summary>
              <span className="history-icon" aria-hidden="true">
                {item.direction === "incoming" ? (
                  <ArrowDownLeft size={20} />
                ) : (
                  <ArrowUpRight size={20} />
                )}
              </span>
              <span className="transaction-history-description">
                <strong>{item.title}</strong>
                <span>
                  {labels[item.status] ?? "Unknown status"} ·{" "}
                  {item.network === "zcash" ? "Zcash" : "Ethereum"}
                </span>
                {item.counterparty && (
                  <span>
                    {item.direction === "incoming" ? "From" : "To"}{" "}
                    {item.counterparty.username
                      ? `@${item.counterparty.username}`
                      : item.counterparty.name}
                  </span>
                )}
                {item.kind === "withdrawal" && item.nft && (
                  <span title={item.nft.to}>
                    To {shortAddress(item.nft.to)}
                  </span>
                )}
              </span>
              <span className="transaction-history-value">
                {item.amount && (
                  <strong>
                    {item.status === "confirmed"
                      ? item.direction === "incoming"
                        ? "+"
                        : "−"
                      : ""}
                    {formatZec(item.amount)} ZEC
                  </strong>
                )}
                <time dateTime={new Date(item.created_at_ms).toISOString()}>
                  {new Date(item.created_at_ms).toLocaleString()}
                </time>
                <span>Details</span>
              </span>
            </summary>
            {item.auction_id && (
              <Link className="text-link" to={`/market/${item.auction_id}`}>
                {item.auction_title ?? "View auction"}
              </Link>
            )}
            <TransactionCard item={item} />
          </details>
        ))}
      </div>
      {(page > 0 || (query.data?.length ?? 0) > 20) && (
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
            disabled={(query.data?.length ?? 0) <= 20}
            onClick={() => setPage(page + 1)}
          >
            Next
          </button>
        </div>
      )}
    </>
  );
}
