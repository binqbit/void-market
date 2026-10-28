import { normalizeZecInput, zecInputError } from "../amounts";
import { useState } from "react";
import { message, post } from "../api/client";
import { ErrorNotice } from "../components/feedback";
import { localDateTime, validAmount } from "./types";
import type { Auction } from "./types";

export function DraftEditor({
  auction,
  onComplete,
}: {
  auction: Auction;
  onComplete: () => void;
}) {
  const [reserve, setReserve] = useState(auction.reserve ?? "");
  const [close, setClose] = useState(() => localDateTime(auction.closes_at_ms));
  const [revision, setRevision] = useState(auction.revision);
  const [consent, setConsent] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const changed =
    reserve !== auction.reserve ||
    close !== localDateTime(auction.closes_at_ms);
  const stale = revision !== auction.revision;

  async function action(kind: "draft" | "publish" | "cancel") {
    setBusy(true);
    setError(null);
    try {
      const updated = await post<Auction>(
        `/v1/auctions/${auction.id}/${kind}`,
        kind === "draft"
          ? {
              revision,
              reserve,
              closes_at_ms:
                close === localDateTime(auction.closes_at_ms)
                  ? auction.closes_at_ms
                  : new Date(close).getTime(),
            }
          : kind === "publish"
            ? { revision, consent }
            : {},
      );
      setRevision(updated.revision);
      setReserve(updated.reserve ?? "");
      setClose(localDateTime(updated.closes_at_ms));
      setConsent(false);
      onComplete();
    } catch (failure) {
      setError(message(failure));
      onComplete();
    } finally {
      setBusy(false);
    }
  }

  function latest() {
    setReserve(auction.reserve ?? "");
    setClose(localDateTime(auction.closes_at_ms));
    setRevision(auction.revision);
    setConsent(false);
  }

  return (
    <section className="auction-action-panel">
      <h2>Review your draft</h2>
      <p>
        The asset stays available until you publish. Published terms cannot be
        edited or cancelled.
      </p>
      {stale && (
        <ErrorNotice retry={latest}>
          This draft changed. Use the latest version before saving or
          publishing.
        </ErrorNotice>
      )}
      <div className="form-stack">
        <label>
          Private reserve price · ZEC
          <input
            inputMode="decimal"
            value={reserve}
            maxLength={32}
            disabled={busy}
            onChange={(event) => {
              setReserve(normalizeZecInput(event.target.value));
              setConsent(false);
            }}
          />
        </label>
        <label>
          Auction ends
          <input
            type="datetime-local"
            value={close}
            disabled={busy}
            onChange={(event) => {
              setClose(event.target.value);
              setConsent(false);
            }}
          />
        </label>
        {zecInputError(reserve, true) && (
          <p className="field-error" role="alert">
            {zecInputError(reserve, true)}
          </p>
        )}
        <button
          className="button"
          disabled={
            busy || stale || !changed || !validAmount(reserve, true) || !close
          }
          onClick={() => void action("draft")}
        >
          Save changes
        </button>
        <p className="field-help">
          Bidding closes one hour before the auction ends. Only approvals are
          available in the final hour, with one shared 10-minute response
          budget. Waiting for others or the service pauses it.
        </p>
        <label className="auction-consent">
          <input
            type="checkbox"
            checked={consent}
            disabled={busy || changed || stale}
            onChange={(event) => setConsent(event.target.checked)}
          />
          <span>
            {auction.execution === "localnet_custody"
              ? "I accept the fixed auction terms and will approve transfer of this NFT to the registered arbiter. After the buyer fully pays the local ZEC invoice, I can approve delivery; my silence until the shared approval budget expires also authorizes delivery."
              : "I agree to lock this asset for the auction, keep its terms fixed, and approve its eventual transfer separately. Payment is not verified by Void Market."}
          </span>
        </label>
        {!auction.wallet_ready && (
          <p className="field-help">
            Your development wallet is preparing. Publication will be available
            shortly.
          </p>
        )}
        <div className="auction-actions">
          <button
            className="button primary"
            disabled={
              busy || !consent || changed || stale || !auction.wallet_ready
            }
            onClick={() => void action("publish")}
          >
            {busy ? "Saving…" : "Publish auction"}
          </button>
          <button
            className="quiet-button"
            disabled={busy}
            onClick={() => void action("cancel")}
          >
            Cancel draft
          </button>
        </div>
      </div>
      {error && <ErrorNotice>{error}</ErrorNotice>}
    </section>
  );
}
