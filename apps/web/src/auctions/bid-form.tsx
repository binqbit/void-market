import { useState } from "react";
import { message, post } from "../api/client";
import { Dialog } from "../components/dialog";
import { ErrorNotice } from "../components/feedback";
import { validAmount } from "./types";
import type { Auction } from "./types";

export function BidForm({
  auction,
  onComplete,
}: {
  auction: Auction;
  onComplete: () => void;
}) {
  const [amount, setAmount] = useState("");
  const [review, setReview] = useState(false);
  const [intent, setIntent] = useState<{
    request_id: string;
    amount: string;
  } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  async function submit() {
    const body =
      intent?.amount === amount
        ? intent
        : { request_id: crypto.randomUUID(), amount };
    setIntent(body);
    setBusy(true);
    setError(null);
    try {
      await post(`/v1/auctions/${auction.id}/bids`, {
        ...body,
        policy_hash: auction.policy_hash,
      });
      setReview(false);
      onComplete();
    } catch (failure) {
      setError(message(failure));
      onComplete();
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="auction-action-panel">
      <h2>Place your private bid</h2>
      <p>Your amount is hidden from other buyers.</p>
      {!auction.wallet_ready && (
        <p className="field-help">
          Your development wallet is preparing. This page will update
          automatically.
        </p>
      )}
      <label>
        Your bid · ZEC
        <input
          inputMode="decimal"
          maxLength={32}
          value={amount}
          placeholder="0.00000000"
          onChange={(event) => setAmount(event.target.value)}
        />
      </label>
      <p className="field-help">
        Up to 8 decimal places. Your admitted amount cannot be changed.
      </p>
      <button
        className="button primary"
        disabled={!auction.wallet_ready || !validAmount(amount)}
        onClick={() => setReview(true)}
      >
        Review bid
      </button>
      {review && (
        <Dialog
          title="Review your bid"
          busy={busy}
          onClose={() => setReview(false)}
        >
          <p>
            <strong>{amount} ZEC</strong> for{" "}
            <strong>{auction.asset.name}</strong>.
          </p>
          <p>
            You have one immutable bid. You may withdraw, but cannot bid again.
            You must approve requests in Messages within your shared 10-minute
            budget.
          </p>
          <p>
            Prices are in ZEC. No funds are collected; the seller confirms
            completion with payment unverified.
          </p>
          {error && <ErrorNotice>{error}</ErrorNotice>}
          <div className="dialog-actions">
            <button
              className="button"
              disabled={busy}
              onClick={() => setReview(false)}
            >
              Back
            </button>
            <button
              className="button primary"
              disabled={busy}
              onClick={() => void submit()}
            >
              {busy ? "Submitting…" : "Confirm bid"}
            </button>
          </div>
        </Dialog>
      )}
    </section>
  );
}
