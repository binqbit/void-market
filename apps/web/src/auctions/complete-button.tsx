import { formatZec } from "../amounts";
import { useState } from "react";
import { post, message } from "../api/client";
import { Dialog } from "../components/dialog";
import { ErrorNotice } from "../components/feedback";
import { useMessages } from "../messages/provider";
import { accountName } from "./types";
import type { Account } from "../api/types";

export function CompleteSaleButton({
  id,
  requestId,
  generation,
  title,
  price,
  buyer,
  paymentStatus = "unverified",
  onComplete,
}: {
  id: string;
  requestId: string;
  generation: number;
  title: string;
  price: string;
  buyer: Account;
  paymentStatus?: string;
  onComplete: () => void;
}) {
  const { refresh } = useMessages();
  const [open, setOpen] = useState(false);
  const [consentFor, setConsentFor] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const context = `${id}:${requestId}:${generation}`;
  const consent = consentFor === context;

  async function complete() {
    if (!consent) return;
    setBusy(true);
    setError(null);
    try {
      await post(`/v1/auctions/${id}/complete`, {
        request_id: requestId,
        generation,
      });
      setOpen(false);
      onComplete();
      await refresh();
    } catch (failure) {
      setError(message(failure));
      onComplete();
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
      <button
        className="button primary"
        onClick={() => {
          setConsentFor(null);
          setError(null);
          setOpen(true);
        }}
      >
        Complete sale
      </button>
      {open && (
        <Dialog
          title="Approve the asset transfer"
          busy={busy}
          onClose={() => setOpen(false)}
        >
          <dl className="auction-review">
            <div>
              <dt>Asset</dt>
              <dd>{title}</dd>
            </div>
            <div>
              <dt>Buyer</dt>
              <dd>{accountName(buyer)}</dd>
            </div>
            <div>
              <dt>Agreed price</dt>
              <dd>{formatZec(price)} ZEC</dd>
            </div>
          </dl>
          <p>
            {paymentStatus === "confirmed"
              ? "The local Zcash payment is confirmed. Approve delivery of the NFT from the arbiter to the buyer. If your remaining time expires, delivery proceeds under the terms accepted at publication."
              : "This transfers ownership inside Void Market. Payment has not been processed or verified. The transfer cannot be undone."}
          </p>
          <label className="auction-consent">
            <input
              type="checkbox"
              checked={consent}
              disabled={busy}
              onChange={(event) =>
                setConsentFor(event.target.checked ? context : null)
              }
            />
            <span>
              {paymentStatus === "confirmed"
                ? "I approve delivery of the paid NFT."
                : "I approve this transfer with payment unverified."}
            </span>
          </label>
          {error && <ErrorNotice>{error}</ErrorNotice>}
          <div className="dialog-actions">
            <button
              className="button"
              disabled={busy}
              onClick={() => setOpen(false)}
            >
              Back
            </button>
            <button
              className="button primary"
              disabled={busy || !consent}
              onClick={() => void complete()}
            >
              {busy ? "Transferring…" : "Approve & transfer"}
            </button>
          </div>
        </Dialog>
      )}
    </>
  );
}
