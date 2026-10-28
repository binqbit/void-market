import { ArrowRight, CheckCircle2, Search } from "lucide-react";
import { useState } from "react";
import type { FormEvent } from "react";
import { ApiError, message, post, request } from "../api/client";
import type { Account, Asset, Transfer } from "../api/types";
import { Dialog } from "../components/dialog";
import { ErrorNotice } from "../components/feedback";
import { clearPending, writePending } from "./pending-transfer";
import type { PendingTransfer } from "./pending-transfer";

export function TransferDialog({
  asset,
  accountId,
  pending,
  onClose,
  onComplete,
  onPending,
}: {
  asset: Asset;
  accountId: string;
  pending: PendingTransfer | null;
  onClose: () => void;
  onComplete: () => void;
  onPending: () => void;
}) {
  const [address, setAddress] = useState(pending?.recipientId ?? "");
  const [recipient, setRecipient] = useState<Account | null>(null);
  const [attempt, setAttempt] = useState<PendingTransfer | null>(pending);
  const [done, setDone] = useState<Transfer | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [checked, setChecked] = useState(false);

  async function findRecipient(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      if (address.trim().toLowerCase() === accountId.toLowerCase())
        throw new Error("Choose another account; you already own this asset.");
      setRecipient(
        await request<Account>(
          `/v1/accounts/${encodeURIComponent(address.trim())}`,
        ),
      );
    } catch (failure) {
      setError(
        failure instanceof Error && !(failure instanceof ApiError)
          ? failure.message
          : message(failure),
      );
    } finally {
      setBusy(false);
    }
  }

  function completed(receipt: Transfer) {
    setDone(receipt);
    try {
      clearPending();
    } catch {
      /* A recorded receipt remains authoritative if storage fails. */
    }
    onComplete();
    onPending();
  }

  async function checkReceipt() {
    if (!attempt) return;
    setBusy(true);
    setError(null);
    try {
      completed(await request<Transfer>(`/v1/transfers/${attempt.requestId}`));
    } catch (failure) {
      setChecked(true);
      setError(
        failure instanceof ApiError && failure.status === 404
          ? "No receipt is available yet. You may retry the same transfer below."
          : message(failure),
      );
    } finally {
      setBusy(false);
    }
  }

  async function transfer() {
    const action = attempt ?? {
      requestId: crypto.randomUUID(),
      accountId,
      assetId: asset.id,
      recipientId: address.trim(),
    };
    setBusy(true);
    setError(null);
    try {
      writePending(action);
    } catch {
      setError(
        "Your browser could not save recovery information. No transfer was submitted. Enable storage and try again.",
      );
      setBusy(false);
      return;
    }
    try {
      setAttempt(action);
      onPending();
      const receipt = await post<Transfer>(`/v1/assets/${asset.id}/transfers`, {
        request_id: action.requestId,
        recipient_address: action.recipientId,
      });
      completed(receipt);
    } catch (failure) {
      if (
        failure instanceof ApiError &&
        failure.status >= 400 &&
        failure.status < 500 &&
        ![401, 408, 429].includes(failure.status)
      ) {
        clearPending();
        setAttempt(null);
        onPending();
        setError(message(failure));
      } else {
        setError(
          "The outcome is not confirmed. Check the receipt before retrying this same transfer.",
        );
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog
      title={done ? "Transfer complete" : "Transfer asset"}
      onClose={onClose}
      busy={busy}
    >
      {done ? (
        <div className="form-stack">
          <CheckCircle2 size={40} className="accent" />
          <p>
            <strong>{asset.name}</strong> now belongs to the recipient.
          </p>
          <p className="field-help">Receipt</p>
          <code className="break-all">{done.id}</code>
          <button className="button primary" onClick={onClose}>
            Done
          </button>
        </div>
      ) : (
        <div className="form-stack">
          <div className="transfer-summary">
            <strong>{asset.name}</strong>
            <span className="muted break-all">{asset.id}</span>
          </div>
          {!attempt && !recipient && (
            <form
              className="form-stack"
              onSubmit={(event) => void findRecipient(event)}
            >
              <label>
                Recipient account address
                <input
                  value={address}
                  onChange={(event) => setAddress(event.target.value)}
                  required
                  autoComplete="off"
                  placeholder="00000000-0000-0000-0000-000000000000"
                  disabled={busy}
                />
              </label>
              <button className="button" disabled={busy}>
                <Search size={16} />
                {busy ? "Finding account…" : "Find recipient"}
              </button>
            </form>
          )}
          {(recipient || attempt) && (
            <>
              <div className="recipient-summary">
                <span className="field-help">NEW OWNER</span>
                <strong>
                  {recipient?.username
                    ? `@${recipient.username}`
                    : (recipient?.name ?? "Recipient address")}
                </strong>
                <code className="break-all">
                  {attempt?.recipientId ?? address}
                </code>
              </div>
              <p>
                This changes ownership inside Void Market. Once recorded, you
                cannot undo it yourself.
              </p>
            </>
          )}
          {error && <ErrorNotice>{error}</ErrorNotice>}
          {attempt ? (
            <>
              <button
                className="button"
                onClick={() => void checkReceipt()}
                disabled={busy}
              >
                Check transfer receipt
              </button>
              {checked && (
                <button
                  className="button primary"
                  onClick={() => void transfer()}
                  disabled={busy}
                >
                  Retry the same transfer <ArrowRight size={17} />
                </button>
              )}
              <p className="field-help">
                You can close this dialog and return to the pending transfer in
                this browser tab.
              </p>
            </>
          ) : (
            recipient && (
              <div className="button-row">
                <button
                  className="button quiet"
                  onClick={() => setRecipient(null)}
                  disabled={busy}
                >
                  Change recipient
                </button>
                <button
                  className="button primary"
                  onClick={() => void transfer()}
                  disabled={busy}
                >
                  {busy ? "Transferring…" : "Confirm transfer"}
                  <ArrowRight size={17} />
                </button>
              </div>
            )
          )}
        </div>
      )}
    </Dialog>
  );
}
