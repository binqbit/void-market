import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { message, post } from "../api/client";
import type { Asset } from "../api/types";
import { useAuctionQuery } from "../auctions/use-auctions";
import { Dialog } from "../components/dialog";
import { ErrorNotice } from "../components/feedback";
import type { DemoStatus, NativeTransaction } from "../demo/types";
import { useMessages } from "../messages/provider";

export function NativeTransferDialog({
  asset,
  accountId,
  onClose,
}: {
  asset: Asset;
  accountId: string;
  onClose: () => void;
}) {
  const demo = useAuctionQuery<DemoStatus>("/v1/demo");
  const { refresh } = useMessages();
  const navigate = useNavigate();
  const [recipient, setRecipient] = useState("");
  const [requestId] = useState(() => crypto.randomUUID());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function prepare() {
    setBusy(true);
    setError(null);
    try {
      await post<NativeTransaction>(`/v1/assets/${asset.id}/native-transfer`, {
        request_id: requestId,
        recipient_address: recipient,
      });
      await refresh();
      onClose();
      navigate("/messages");
    } catch (failure) {
      setError(message(failure));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog title="Transfer NFT" onClose={onClose} busy={busy}>
      <div className="form-stack">
        <strong>{asset.name}</strong>
        <p>
          The recipient's local Ethereum address is filled in automatically.
          Review and approve the exact transaction in Messages. Ownership
          changes only after confirmation.
        </p>

        <label>
          Recipient
          <select
            value={recipient}
            onChange={(event) => setRecipient(event.target.value)}
            disabled={busy}
          >
            <option value="">Choose an account</option>
            {demo.data?.actors
              .filter((actor) => actor.account_id !== accountId)
              .map((actor) => (
                <option key={actor.account_id} value={actor.account_id}>
                  {actor.name}
                </option>
              ))}
          </select>
        </label>
        {demo.error && (
          <ErrorNotice retry={demo.refresh}>{demo.error}</ErrorNotice>
        )}
        <button
          className="button primary"
          disabled={busy || !recipient}
          onClick={() => void prepare()}
        >
          {busy ? "Preparing…" : "Prepare transfer"}
        </button>
        {error && <ErrorNotice>{error}</ErrorNotice>}
      </div>
    </Dialog>
  );
}
