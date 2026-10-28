import { useState } from "react";
import { message, post } from "../api/client";
import { useRemote } from "../api/use-remote";
import type { Asset } from "../api/types";
import { CopyAddress } from "../components/copy-address";
import { Dialog } from "../components/dialog";
import { ErrorNotice, Loading } from "../components/feedback";

export function DepositDialog({
  onClose,
  onComplete,
}: {
  onClose: () => void;
  onComplete: () => void;
}) {
  const wallet = useRemote<{ address: string; chain_id: number }>(
    "/v1/me/nft-deposit",
  );
  const [transaction, setTransaction] = useState("");
  const [collection, setCollection] = useState("");
  const [token, setToken] = useState("");
  const [name, setName] = useState("");
  const [requestId] = useState(() => crypto.randomUUID());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const valid =
    /^0x[0-9a-fA-F]{64}$/.test(transaction.trim()) &&
    /^0x[0-9a-fA-F]{40}$/.test(collection.trim()) &&
    /^(0|[1-9]\d{0,77})$/.test(token);

  async function confirm() {
    setBusy(true);
    setError(null);
    try {
      await post<Asset>("/v1/nft-deposits", {
        request_id: requestId,
        transaction_id: transaction.trim(),
        collection: collection.trim(),
        token_id: token,
        name: name.trim() || null,
      });
      onComplete();
      onClose();
    } catch (failure) {
      setError(message(failure));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog title="Deposit NFT" onClose={onClose} busy={busy}>
      <div className="nft-dialog">
        {wallet.loading ? (
          <Loading>Loading your deposit address…</Loading>
        ) : wallet.error ? (
          <ErrorNotice retry={wallet.retry}>{wallet.error}</ErrorNotice>
        ) : (
          wallet.data && (
            <>
              <p>Send your ERC-721 NFT to this personal deposit address.</p>
              <CopyAddress
                address={wallet.data.address}
                label="Copy Ethereum deposit address"
              />
              <p className="field-help">
                Ethereum localnet · Chain {wallet.data.chain_id}. Send only on
                this network.
              </p>
              <hr />
              <p>
                After sending, enter the transfer details. Your NFT appears in
                the portfolio once the deposit is verified.
              </p>
              <label>
                Transaction hash
                <input
                  value={transaction}
                  onChange={(event) => setTransaction(event.target.value)}
                  autoComplete="off"
                  spellCheck={false}
                  placeholder="0x…"
                  maxLength={66}
                  disabled={busy}
                />
              </label>
              <label>
                NFT contract address
                <input
                  value={collection}
                  onChange={(event) => setCollection(event.target.value)}
                  autoComplete="off"
                  spellCheck={false}
                  placeholder="0x…"
                  maxLength={42}
                  disabled={busy}
                />
              </label>
              <label>
                Token ID
                <input
                  value={token}
                  onChange={(event) => setToken(event.target.value)}
                  inputMode="numeric"
                  placeholder="For example, 42"
                  maxLength={78}
                  disabled={busy}
                />
              </label>
              <label>
                Name{" "}
                <span className="field-help">
                  Optional · used for a newly added NFT
                </span>
                <input
                  value={name}
                  onChange={(event) => setName(event.target.value)}
                  placeholder="My NFT"
                  maxLength={128}
                  disabled={busy}
                />
              </label>
              {error && <ErrorNotice>{error}</ErrorNotice>}
              <button
                className="button primary"
                disabled={busy || !valid}
                onClick={() => void confirm()}
              >
                {busy ? "Checking deposit…" : "Confirm deposit"}
              </button>
            </>
          )
        )}
      </div>
    </Dialog>
  );
}
