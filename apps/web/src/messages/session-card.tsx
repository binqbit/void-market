import { useState } from "react";
import { Link } from "react-router-dom";
import { post, message } from "../api/client";
import { Dialog } from "../components/dialog";
import { ErrorNotice } from "../components/feedback";
import { useMessages } from "./provider";
import type { MessageSnapshot } from "./types";

export function SessionCard({
  item,
}: {
  item: MessageSnapshot["sessions"][number];
}) {
  const { refresh } = useMessages();
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const active =
    item.participation_status === "active" &&
    ["initializing", "active"].includes(item.status);
  const status =
    item.participation_status === "disqualified"
      ? "Disqualified — approval time expired"
      : item.participation_status === "withdrawn"
        ? "Bid withdrawn"
        : item.status === "completed"
          ? "Comparison completed"
          : item.status === "interrupted"
            ? "Session interrupted — no disqualification"
            : item.status === "initializing"
              ? "Preparing your development wallet"
              : item.waiting_until_ms
                ? `Waiting for bidding to close at ${new Date(item.waiting_until_ms).toLocaleString()}`
                : "Waiting for the arbiter or another participant";

  async function withdraw() {
    setBusy(true);
    setError(null);
    try {
      await post(`/v1/approval-sessions/${item.id}/withdraw`, {});
      setConfirm(false);
      await refresh();
    } catch (failure) {
      setError(message(failure));
    } finally {
      setBusy(false);
    }
  }

  return (
    <article className="message-session">
      <div>
        <h3>{item.title}</h3>
        <p>{status}</p>
        {item.auction_id && (
          <Link className="text-link" to={`/market/${item.auction_id}`}>
            View auction
          </Link>
        )}
      </div>
      {active && item.can_withdraw !== false && (
        <button className="quiet-button" onClick={() => setConfirm(true)}>
          Withdraw bid
        </button>
      )}
      {confirm && (
        <Dialog
          title="Withdraw your bid?"
          busy={busy}
          onClose={() => setConfirm(false)}
        >
          <p>
            You will leave <strong>{item.title}</strong>. Your pending approvals
            in this session will be cancelled. This cannot be undone.
          </p>
          {error && <ErrorNotice>{error}</ErrorNotice>}
          <div className="dialog-actions">
            <button
              className="button"
              disabled={busy}
              onClick={() => setConfirm(false)}
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
    </article>
  );
}
