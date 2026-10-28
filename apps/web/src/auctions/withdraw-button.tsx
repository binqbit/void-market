import { useState } from "react";
import { post, message } from "../api/client";
import { Dialog } from "../components/dialog";
import { ErrorNotice } from "../components/feedback";
import { useMessages } from "../messages/provider";

export function WithdrawBidButton({
  id,
  title,
  onComplete,
}: {
  id: string;
  title: string;
  onComplete: () => void;
}) {
  const { refresh } = useMessages();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  async function withdraw() {
    setBusy(true);
    setError(null);
    try {
      await post(`/v1/auctions/${id}/withdraw`, {});
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
      <button className="quiet-button" onClick={() => setOpen(true)}>
        Withdraw bid
      </button>
      {open && (
        <Dialog
          title="Withdraw your bid?"
          busy={busy}
          onClose={() => setOpen(false)}
        >
          <p>
            You will leave <strong>{title}</strong>. Pending approvals will be
            cancelled and you cannot bid again in this auction. Other buyers may
            continue.
          </p>
          {error && <ErrorNotice>{error}</ErrorNotice>}
          <div className="dialog-actions">
            <button
              className="button"
              disabled={busy}
              onClick={() => setOpen(false)}
            >
              Keep participating
            </button>
            <button
              className="button"
              disabled={busy}
              onClick={() => void withdraw()}
            >
              {busy ? "Withdrawing…" : "Withdraw bid"}
            </button>
          </div>
        </Dialog>
      )}
    </>
  );
}
