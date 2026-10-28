import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { message, post } from "../api/client";
import type { Asset } from "../api/types";
import { Dialog } from "../components/dialog";
import { ErrorNotice } from "../components/feedback";
import type { NativeTransaction } from "../demo/types";
import { useMessages } from "../messages/provider";

export function WithdrawDialog({
  asset,
  onClose,
}: {
  asset: Asset;
  onClose: () => void;
}) {
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
      await post<NativeTransaction>(`/v1/assets/${asset.id}/withdraw`, {
        request_id: requestId,
        recipient_address: recipient.trim(),
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
    <Dialog title="Withdraw NFT" onClose={onClose} busy={busy}>
      <div className="nft-dialog">
        <strong>{asset.name}</strong>
        <p>
          Send this NFT to an Ethereum wallet. Review the destination and
          approve the transaction in Messages. The NFT leaves your portfolio
          after confirmation.
        </p>

        <label>
          Destination Ethereum address
          <input
            type="text"
            value={recipient}
            onChange={(event) => setRecipient(event.target.value)}
            placeholder="0x…"
            autoComplete="off"
            spellCheck={false}
            maxLength={42}
            disabled={busy}
          />
        </label>
        <p className="field-help">
          Use an address on Ethereum localnet (chain 31337).
        </p>
        <button
          className="button primary"
          disabled={
            busy ||
            !/^0x[0-9a-fA-F]{40}$/.test(recipient.trim()) ||
            /^0x0{40}$/.test(recipient.trim())
          }
          onClick={() => void prepare()}
        >
          {busy ? "Preparing…" : "Review withdrawal"}
        </button>
        {error && <ErrorNotice>{error}</ErrorNotice>}
      </div>
    </Dialog>
  );
}
