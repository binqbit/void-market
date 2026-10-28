import { normalizeZecInput, zecInputError } from "../amounts";
import { useState } from "react";
import type { FormEvent } from "react";
import { useNavigate } from "react-router-dom";
import type { Asset } from "../api/types";
import { message, post } from "../api/client";
import { Dialog } from "../components/dialog";
import { ErrorNotice } from "../components/feedback";
import { localDateTime, validAmount } from "./types";
import type { Auction } from "./types";

export function CreateAuctionDialog({
  asset,
  onClose,
}: {
  asset: Asset;
  onClose: () => void;
}) {
  const navigate = useNavigate();
  const [id] = useState(() => crypto.randomUUID());
  const [reserve, setReserve] = useState("");
  const [close, setClose] = useState(() =>
    localDateTime(Date.now() + 86400000),
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function save(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const auction = await post<Auction>("/v1/auctions", {
        request_id: id,
        asset_id: asset.id,
        reserve,
        closes_at_ms: new Date(close).getTime(),
      });
      navigate(`/market/${auction.id}`);
      onClose();
    } catch (failure) {
      setError(message(failure));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog title="Create an auction" busy={busy} onClose={onClose}>
      <p>
        Prepare a sale for <strong>{asset.name}</strong>. You will review the
        terms before publishing.
      </p>
      <form className="form-stack" onSubmit={(event) => void save(event)}>
        <label>
          Private reserve price · ZEC
          <input
            required
            inputMode="decimal"
            maxLength={32}
            placeholder="0.0"
            value={reserve}
            disabled={busy}
            onChange={(event) =>
              setReserve(normalizeZecInput(event.target.value))
            }
          />
        </label>
        {zecInputError(reserve, true) && (
          <p className="field-error" role="alert">
            {zecInputError(reserve, true)}
          </p>
        )}
        <p className="field-help">
          Up to 8 decimal places. Hidden from buyers. In this development
          version, Void Market holds the keys and processes the amount.
        </p>
        <label>
          Auction ends
          <input
            type="datetime-local"
            required
            value={close}
            disabled={busy}
            onChange={(event) => setClose(event.target.value)}
          />
        </label>
        <p className="field-help">
          New bids close one hour before the auction ends. The final hour is for
          approvals only. Up to 3 buyers; each participant has one shared
          10-minute response budget.
        </p>
        {error && <ErrorNotice>{error}</ErrorNotice>}
        <button
          className="button primary"
          disabled={busy || !validAmount(reserve, true) || !close}
        >
          {busy ? "Saving…" : "Save draft"}
        </button>
      </form>
    </Dialog>
  );
}
