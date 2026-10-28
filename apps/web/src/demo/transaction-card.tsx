import { formatZec, normalizeZecInput, zecInputError } from "../amounts";
import { ArrowUpRight, CheckCircle2, Clock3, LockKeyhole } from "lucide-react";
import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { post, request, message } from "../api/client";
import type { Auction } from "../auctions/types";
import { validAmount } from "../auctions/types";
import { Dialog } from "../components/dialog";
import { ErrorNotice } from "../components/feedback";
import { formatTime, remainingTime } from "../messages/countdown";
import { useMessages } from "../messages/provider";
import { shortAddress } from "./types";
import type { NativeTransaction } from "./types";

const progress: Record<string, string> = {
  preparing: "Preparing the transaction…",
  ready: "Ready for your approval",
  approved: "Approval received. Building the transaction…",
  building: "Creating the native proof and signature…",
  signed: "Signed. Preparing to broadcast…",
  submitted: "Waiting for confirmations…",
  confirmed: "Confirmed on the local network",
  failed: "This transaction needs attention",
  reconcile: "Checking the existing transaction…",
  cancelled: "Cancelled",
};

export function TransactionCard({ item }: { item: NativeTransaction }) {
  const { refresh, error: messagesError } = useMessages();
  const [review, setReview] = useState<NativeTransaction | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [elapsed, setElapsed] = useState(0);
  const [partial, setPartial] = useState("");
  const retryClaim =
    item.status === "failed" && item.kind === "airdrop" && item.can_retry;
  useEffect(() => {
    setElapsed(0);
    if (!item.timer_running || messagesError) return;
    const start = performance.now();
    const timer = window.setInterval(
      () => setElapsed(performance.now() - start),
      250,
    );
    return () => window.clearInterval(timer);
  }, [item.remaining_ms, item.timer_running, messagesError]);

  async function approve() {
    if (
      !review ||
      review.revision !== item.revision ||
      !item.can_approve ||
      messagesError
    )
      return;
    setBusy(true);
    setError(null);
    try {
      await post(`/v1/transactions/${review.id}/approve`, {
        revision: review.revision,
      });
      setReview(null);
      await refresh();
    } catch (failure) {
      setError(message(failure));
    } finally {
      setBusy(false);
    }
  }

  async function preparePayment(amount?: string) {
    if (!item.auction_id) return;
    setBusy(true);
    setError(null);
    try {
      const auction = await request<Auction>(`/v1/auctions/${item.auction_id}`);
      await post(`/v1/auctions/${item.auction_id}/payments`, {
        request_id: crypto.randomUUID(),
        generation: auction.generation,
        amount: amount ?? null,
      });
      setPartial("");
      await refresh();
    } catch (failure) {
      setError(message(failure));
    } finally {
      setBusy(false);
    }
  }

  async function cancelTransfer() {
    setBusy(true);
    setError(null);
    try {
      await post(`/v1/transactions/${item.id}/cancel`, {});
      await refresh();
    } catch (failure) {
      setError(message(failure));
    } finally {
      setBusy(false);
    }
  }

  return (
    <article
      className={`native-transaction panel ${item.can_approve ? "needs-approval" : ""}`}
    >
      <div className="message-content">
        <div className="message-topline">
          <span className="eyebrow">
            {item.network === "zcash"
              ? "Zcash · Ironwood regtest"
              : "Ethereum · localnet"}
          </span>
          {item.status === "confirmed" ? (
            <CheckCircle2 size={18} aria-hidden="true" />
          ) : (
            <Clock3 size={18} aria-hidden="true" />
          )}
        </div>
        <h3>{item.title}</h3>
        {item.counterparty && (
          <p className="field-help">
            {item.direction === "incoming" ? "From" : "To"}{" "}
            {item.counterparty.username
              ? `@${item.counterparty.username}`
              : item.counterparty.name}
          </p>
        )}
        <p className="message-description">
          {progress[item.status] ?? "Checking transaction status…"}
        </p>
        {item.amount && (
          <p className="native-amount">
            {formatZec(item.amount)} <span>ZEC</span>
          </p>
        )}
        <div className="native-links">
          {item.auction_id && (
            <Link className="text-link" to={`/market/${item.auction_id}`}>
              View auction <ArrowUpRight size={14} />
            </Link>
          )}
          {item.transaction_id && (
            <code title={item.transaction_id}>
              {shortAddress(item.transaction_id)}
            </code>
          )}
        </div>
      </div>
      <div className="message-controls">
        {item.remaining_ms !== null && item.can_approve && (
          <>
            <div className="message-time">
              <Clock3 size={15} aria-hidden="true" />
              <strong>
                {messagesError
                  ? "Reconnecting…"
                  : formatTime(
                      remainingTime(
                        item.remaining_ms,
                        item.timer_running,
                        elapsed,
                      ),
                    )}
              </strong>
              <span>{item.timer_running ? "remaining" : "paused"}</span>
            </div>
            <span className="message-budget">Shared auction budget</span>
          </>
        )}
        {item.can_approve && (
          <button
            className="button primary"
            disabled={Boolean(messagesError)}
            onClick={() => {
              setReview(item);
              setError(null);
            }}
          >
            <LockKeyhole size={16} /> Review & approve
          </button>
        )}
        {retryClaim && (
          <Link className="button" to="/demo">
            Request local ZEC again
          </Link>
        )}
        {item.can_retry && item.kind === "mint" && (
          <Link className="button" to="/demo">
            Get another local NFT
          </Link>
        )}
        {item.can_retry &&
          item.status === "failed" &&
          item.kind === "payment" &&
          item.direction !== "incoming" && (
            <button
              className="button"
              disabled={busy}
              onClick={() => void preparePayment()}
            >
              Prepare payment again
            </button>
          )}
        {(item.kind === "transfer" || item.kind === "withdrawal") &&
          item.direction === "outgoing" &&
          item.can_cancel !== false &&
          ["preparing", "ready", "failed"].includes(item.status) && (
            <button
              className="quiet-button"
              disabled={busy}
              onClick={() => void cancelTransfer()}
            >
              {item.kind === "withdrawal"
                ? "Cancel withdrawal"
                : "Cancel transfer"}
            </button>
          )}
      </div>
      {item.error && (
        <ErrorNotice>
          {item.kind === "airdrop" &&
          item.error ===
            "The signed payment expired without confirmation. Prepare and review a fresh payment."
            ? "This ZEC claim expired without confirmation. Open Demo to request local ZEC again."
            : item.error}
        </ErrorNotice>
      )}
      {error && <ErrorNotice>{error}</ErrorNotice>}
      {item.kind === "payment" && item.can_approve && (
        <details className="partial-payment">
          <summary>Test a partial payment</summary>
          <p>
            The remaining amount must be paid before your approval time expires.
            This does not complete the purchase.
          </p>
          <label>
            Amount · ZEC
            <input
              inputMode="decimal"
              value={partial}
              maxLength={32}
              onChange={(event) =>
                setPartial(normalizeZecInput(event.target.value))
              }
              placeholder="0.5"
            />
          </label>
          {zecInputError(partial) && (
            <p className="field-error" role="alert">
              {zecInputError(partial)}
            </p>
          )}
          <button
            className="button"
            disabled={busy || !validAmount(partial)}
            onClick={() => void preparePayment(partial)}
          >
            Prepare this amount
          </button>
        </details>
      )}
      {item.confirmation && (
        <details className="partial-payment">
          <summary>Confirmed transaction receipt</summary>
          <p className="break-all">Transaction: {item.transaction_id}</p>
          <p className="break-all">Block: {item.confirmation.block_hash}</p>
          {item.confirmation.confirmations && (
            <p>
              {item.confirmation.confirmations} confirmations at verification
            </p>
          )}
        </details>
      )}
      {review && (
        <Dialog
          title={review.title}
          busy={busy}
          onClose={() => setReview(null)}
        >
          <p>
            This is a real transaction on a local test network. The system has
            prepared its destination and contents.
          </p>
          <dl className="auction-review">
            <div>
              <dt>Network</dt>
              <dd>
                {review.network === "zcash"
                  ? "Zcash regtest · Ironwood"
                  : "Ethereum localnet · 31337"}
              </dd>
            </div>
            {review.amount && (
              <>
                <div>
                  <dt>Payment</dt>
                  <dd>{formatZec(review.amount)} ZEC</dd>
                </div>
                <div>
                  <dt>Network fee</dt>
                  <dd>
                    {review.fee === null
                      ? "Unavailable"
                      : formatZec(review.fee)}{" "}
                    ZEC
                  </dd>
                </div>
              </>
            )}
            <div>
              <dt>Destination</dt>
              <dd className="break-all">
                {review.nft?.to ?? review.recipient}
              </dd>
            </div>
            {review.nft && (
              <div>
                <dt>Collection</dt>
                <dd className="break-all">{review.nft.collection}</dd>
              </div>
            )}
            {review.nft && (
              <div>
                <dt>NFT token</dt>
                <dd className="break-all">#{review.nft.token_id}</dd>
              </div>
            )}
            {review.ethereum_transaction && (
              <div>
                <dt>Maximum network fee</dt>
                <dd>
                  {formatEther(
                    BigInt(review.ethereum_transaction.gas) *
                      BigInt(review.ethereum_transaction.gasPrice),
                  )}{" "}
                  ETH
                </dd>
              </div>
            )}
          </dl>
          {review.kind === "deposit" && (
            <p>
              The NFT stays with the assigned arbiter until delivery or return.
              After full verified payment, the seller's missed approval deadline
              counts as consent to deliver.
            </p>
          )}
          {review.kind === "withdrawal" && (
            <p>
              This sends the NFT out of Void Market to the address above. After
              confirmation, it leaves your portfolio. Check the address before
              approving.
            </p>
          )}
          {review.kind === "payment" && (
            <p>
              Authorizing payment locks the selected winner. Your ZEC transfer
              is not automatically reversible. A partial payment requires a
              top-up.
            </p>
          )}
          {review.revision !== item.revision && (
            <ErrorNotice>
              The request changed. Close this review and open the current
              request.
            </ErrorNotice>
          )}
          {error && <ErrorNotice>{error}</ErrorNotice>}
          <div className="dialog-actions">
            <button
              className="button"
              disabled={busy}
              onClick={() => setReview(null)}
            >
              Back
            </button>
            <button
              className="button primary"
              disabled={
                busy ||
                Boolean(messagesError) ||
                !item.can_approve ||
                item.revision !== review.revision
              }
              onClick={() => void approve()}
            >
              {busy ? "Approving…" : "Approve & send"}
            </button>
          </div>
        </Dialog>
      )}
    </article>
  );
}

function formatEther(value: bigint): string {
  return `${value / 1000000000000000000n}.${(value % 1000000000000000000n).toString().padStart(18, "0").replace(/0+$/, "") || "0"}`;
}
