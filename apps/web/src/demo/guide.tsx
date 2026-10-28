import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { post, message } from "../api/client";
import type { Asset } from "../api/types";
import type { Auction } from "../auctions/types";
import { BidForm } from "../auctions/bid-form";
import { useAuctionQuery } from "../auctions/use-auctions";
import { ErrorNotice } from "../components/feedback";

const key = "void.demo.auction";
function savedAuction() {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

export function DemoGuide({
  alias,
  assets,
}: {
  alias: string | undefined;
  assets: Asset[];
}) {
  const navigate = useNavigate();
  const [id, setId] = useState(savedAuction);
  const query = useAuctionQuery<Auction>(id ? `/v1/auctions/${id}` : null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const auction = query.data;
  const nft = assets.find(
    (asset) =>
      asset.nft && !asset.active_auction_id && !asset.pending_transaction_id,
  );

  async function prepare() {
    if (!nft) return;
    setBusy(true);
    setError(null);
    try {
      const draft = await post<Auction>("/v1/auctions", {
        request_id: crypto.randomUUID(),
        asset_id: nft.id,
        reserve: "1.00000000",
        closes_at_ms: Date.now() + 30 * 60 * 1000,
      });
      localStorage.setItem(key, draft.id);
      setId(draft.id);
      navigate(`/market/${draft.id}`);
    } catch (failure) {
      setError(message(failure));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="demo-guide panel">
      <span className="eyebrow">A complete local-chain walkthrough</span>
      <h2>One NFT. Three accounts. A real sale.</h2>
      <ol className="demo-steps">
        <li>
          <strong>Alice prepares the asset</strong>
          <p>
            Switch to Alice and select Get NFT. The collection mints a random
            collectible into her Ethereum wallet.
          </p>
        </li>
        <li>
          <strong>Alice publishes an auction</strong>
          <p>
            Use a private reserve of 1 ZEC. Review publication terms, then
            separately approve the NFT deposit to the assigned arbiter.
          </p>
          {alias === "alice" && !id && (
            <button
              className="button primary"
              disabled={busy || !nft}
              onClick={() => void prepare()}
            >
              {busy ? "Preparing…" : "Create walkthrough auction"}
            </button>
          )}
        </li>
        <li>
          <strong>Bob and Daniel place their bids</strong>
          <p>
            Claim local ZEC for each buyer. Bob bids 1.50 ZEC; Daniel bids 1.25
            ZEC. The draft allows 30 minutes for setup and bidding. You can
            adjust its closing time before publishing.
          </p>
        </li>
        <li>
          <strong>Approve each private check</strong>
          <p>
            Switch between the three accounts and follow Messages. HE
            computation may take several minutes; service work pauses the
            approval clock.
          </p>
          <Link className="text-link" to="/messages">
            Open Messages
          </Link>
        </li>
        <li>
          <strong>Bob pays, then Alice approves delivery</strong>
          <p>
            Bob reviews a real shielded Zcash payment. After full payment is
            confirmed, Alice can approve the NFT transfer. If she lets her
            remaining time expire, the agreed timeout rule authorizes delivery.
          </p>
        </li>
        <li>
          <strong>Verify the result</strong>
          <p>
            Bob's portfolio shows the NFT. The transaction history shows the
            Zcash payment and Ethereum delivery with their actual transaction
            IDs.
          </p>
          <Link className="text-link" to="/portfolio">
            Open portfolio
          </Link>
        </li>
      </ol>
      {error && <ErrorNotice>{error}</ErrorNotice>}
      {id && (
        <div className="demo-current-auction">
          <Link className="button" to={`/market/${id}`}>
            Open walkthrough auction
          </Link>
          <button
            className="quiet-button"
            onClick={() => {
              localStorage.removeItem(key);
              setId(null);
            }}
          >
            Choose a new walkthrough
          </button>
          {auction && (
            <p>
              Current stage:{" "}
              <strong>{auction.status.replaceAll("_", " ")}</strong>
            </p>
          )}
          {auction?.actions.bid && (alias === "bob" || alias === "daniel") && (
            <BidForm
              key={alias}
              auction={auction}
              initialAmount={alias === "bob" ? "1.50000000" : "1.25000000"}
              onComplete={query.refresh}
            />
          )}
        </div>
      )}
    </section>
  );
}
