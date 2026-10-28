import { ArrowLeft, Clock3, LockKeyhole, ShieldCheck } from "lucide-react";
import { Link, useParams } from "react-router-dom";
import { useSession } from "../auth/session";
import { ErrorNotice, Loading } from "../components/feedback";
import { AssetImage } from "../components/asset-image";
import { useAuctionQuery } from "../auctions/use-auctions";
import { accountName, statusLabels } from "../auctions/types";
import type { Auction } from "../auctions/types";
import { BidForm } from "../auctions/bid-form";
import { CompleteSaleButton } from "../auctions/complete-button";
import { DraftEditor } from "../auctions/draft-editor";
import { WithdrawBidButton } from "../auctions/withdraw-button";

const reasons: Record<string, string> = {
  no_remaining_buyers:
    "No participating buyers remain. The asset stays with the seller.",
  reserve_not_met:
    "No remaining bid met the private reserve. The asset stays with the seller.",
  seller_approval_expired:
    "The seller's approval time expired. The asset stays with the seller.",
  comparison_interrupted:
    "The arbiter's check was interrupted. No buyer was disqualified for this service failure.",
  invalid_comparison_result:
    "The check could not be accepted. Ownership has not changed.",
  inconsistent_transcript:
    "The comparison results could not be reconciled. Ownership has not changed.",
};

export function AuctionPage() {
  const { id } = useParams();
  const session = useSession();
  const query = useAuctionQuery<Auction>(
    session.loading ? null : `/v1/auctions/${id}`,
  );
  if (session.loading || query.loading)
    return <Loading>Loading auction…</Loading>;
  if (!query.data)
    return (
      <div className="page-width page-content">
        <ErrorNotice retry={query.refresh}>
          {query.error ?? "This auction is unavailable."}
        </ErrorNotice>
      </div>
    );
  const auction = query.data;
  const own = auction.participation;
  return (
    <div className="page-width page-content auction-page">
      <Link className="text-link" to="/market">
        <ArrowLeft size={16} /> Back to market
      </Link>
      {query.error && (
        <ErrorNotice retry={query.refresh}>{query.error}</ErrorNotice>
      )}
      <div className="auction-layout">
        <section className="auction-asset">
          <AssetImage image={auction.asset.image} name={auction.asset.name} />
          <h1>{auction.asset.name}</h1>
          <p>{auction.asset.description || "No description provided."}</p>
          <span className="field-help">
            Seller {accountName(auction.seller)}
          </span>
        </section>
        <div className="auction-sidebar">
          <section className="auction-summary panel">
            <span className={`auction-status status-${auction.status}`}>
              {statusLabels[auction.status]}
            </span>
            <h2>
              {auction.status === "open"
                ? "A private bid. A clear decision."
                : statusLabels[auction.status]}
            </h2>
            <div className="auction-card-meta">
              <Clock3 size={16} />
              Bidding closes {new Date(auction.closes_at_ms).toLocaleString()}
            </div>
            <div className="auction-card-meta">
              <LockKeyhole size={16} />
              {auction.admitted_buyers} of {auction.buyer_limit} buyers admitted
            </div>
            {auction.reserve !== null && (
              <p>
                Your private reserve <strong>{auction.reserve} ZEC</strong>
              </p>
            )}
            {auction.your_bid !== null && (
              <p>
                Your bid <strong>{auction.your_bid} ZEC</strong>
              </p>
            )}
            {own?.status === "withdrawn" && (
              <p className="notice">
                You withdrew. You cannot bid again in this auction.
              </p>
            )}
            {own?.status === "disqualified" && (
              <p className="notice">
                Your approval budget expired. You are disqualified from this
                auction.
              </p>
            )}
            {auction.reason && (
              <p>{reasons[auction.reason] ?? "This auction has ended."}</p>
            )}
            {auction.status === "matching" && (
              <>
                <p>
                  Private checks may take a few minutes. Your approval time
                  pauses while the service is working.
                </p>
                <p>
                  {auction.checks_completed} of{" "}
                  {auction.checks_total || "the scheduled"} comparisons
                  complete.
                  {auction.generation > 1
                    ? ` Checking the remaining buyers in round ${auction.generation}.`
                    : ""}
                </p>
                {session.profile && (
                  <Link className="button" to="/messages">
                    Open Messages
                  </Link>
                )}
              </>
            )}
            {auction.winner && (
              <p>
                Selected buyer <strong>{accountName(auction.winner)}</strong>
              </p>
            )}
            {auction.agreed_price && (
              <p>
                Agreed price <strong>{auction.agreed_price} ZEC</strong>
              </p>
            )}
            {auction.status === "awaiting_completion" && (
              <p>
                The asset is locked until the seller approves the transfer.
                Payment remains unverified.
              </p>
            )}
            {auction.status === "completed" && (
              <p>
                Ownership was transferred inside Void Market. Payment remains
                unverified.
              </p>
            )}
            {auction.actions.complete &&
              auction.winner &&
              auction.agreed_price &&
              auction.completion_request_id && (
                <CompleteSaleButton
                  id={auction.id}
                  requestId={auction.completion_request_id}
                  generation={auction.generation}
                  title={auction.asset.name}
                  price={auction.agreed_price}
                  buyer={auction.winner}
                  onComplete={query.refresh}
                />
              )}
            {auction.actions.withdraw && (
              <WithdrawBidButton
                id={auction.id}
                title={auction.asset.name}
                onComplete={query.refresh}
              />
            )}
          </section>
          {auction.actions.edit && (
            <DraftEditor auction={auction} onComplete={query.refresh} />
          )}
          {auction.actions.bid && (
            <BidForm auction={auction} onComplete={query.refresh} />
          )}
          {!session.profile && auction.status === "open" && (
            <Link className="button primary" to="/login">
              Sign in to bid
            </Link>
          )}
          <div className="custody-note">
            <ShieldCheck size={17} />
            <span>
              Development custody · Prices in ZEC. No blockchain payments or
              external asset transfers.
            </span>
          </div>
          <details className="auction-rules">
            <summary>Auction rules</summary>
            <ul>
              <li>One immutable bid per buyer. The seller cannot bid.</li>
              <li>A bid qualifies when it is at least the hidden reserve.</li>
              <li>Equal bids favor the earlier admitted bid.</li>
              <li>
                Up to {auction.buyer_limit} lifetime-admitted buyers. Withdrawal
                does not free a place.
              </li>
              <li>
                You have 10 minutes total for your required approvals across the
                auction. Service waits pause the budget.
              </li>
              <li>
                Withdrawal or approval expiry may select a different surviving
                buyer. Bids are not funded or guaranteed.
              </li>
              <li>
                The seller confirms the final internal transfer with payment
                unverified.
              </li>
            </ul>
          </details>
        </div>
      </div>
    </div>
  );
}
