import {
  Check,
  CheckCheck,
  Clock3,
  LoaderCircle,
  ShieldCheck,
} from "lucide-react";
import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { message, post } from "../api/client";
import { Dialog } from "../components/dialog";
import { ErrorNotice } from "../components/feedback";
import { formatTime, remainingTime } from "./countdown";
import { useMessages } from "./provider";
import { stages } from "./types";
import type { ApprovalMessage } from "./types";

export function MessageCard({ item }: { item: ApprovalMessage }) {
  const { refresh, error: connectionError } = useMessages();
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [withdraw, setWithdraw] = useState(false);
  const [anchor, setAnchor] = useState(() => performance.now());
  const [elapsed, setElapsed] = useState(0);
  const stage = stages[item.stage];
  const early =
    item.required_from_ms != null && Date.now() < item.required_from_ms;
  const biddingOpen =
    item.closes_at_ms != null && Date.now() < item.closes_at_ms;
  const actionable =
    item.status === "ready" &&
    item.participant_status === "active" &&
    item.session_status === "active";
  const left = remainingTime(item.remaining_ms, item.timer_running, elapsed);
  const deadlineReached = left === 0 && !connectionError;

  useEffect(() => {
    setAnchor(performance.now());
    setElapsed(0);
  }, [item.remaining_ms, item.timer_running]);
  useEffect(() => {
    if (!item.timer_running || connectionError) return;
    const timer = window.setInterval(
      () => setElapsed(performance.now() - anchor),
      250,
    );
    return () => window.clearInterval(timer);
  }, [anchor, item.timer_running, connectionError]);

  async function approve() {
    if (busy) return;
    setBusy(true);
    setFailure(null);
    try {
      await post(`/v1/messages/${item.id}/approve`, {
        request_id: item.request_id,
        revision: item.revision,
      });
      await refresh();
    } catch (error) {
      setFailure(message(error));
      await refresh();
    } finally {
      setBusy(false);
    }
  }

  async function leave() {
    setBusy(true);
    setFailure(null);
    try {
      await post(`/v1/approval-sessions/${item.session_id}/withdraw`, {});
      setWithdraw(false);
      await refresh();
    } catch (error) {
      setFailure(message(error));
    } finally {
      setBusy(false);
    }
  }

  const status =
    item.participant_status === "disqualified"
      ? "Disqualified — time budget expired"
      : item.participant_status === "withdrawn"
        ? "Bid withdrawn"
        : ((
            {
              completed: "Completed",
              responded: biddingOpen
                ? "Prepared — waiting for bidding to close"
                : "Waiting for the arbiter",
              approved: "Approved — processing",
              queued: "Approved — processing",
              processing: "Approved — processing",
              response_pending: "Approved — processing",
              blocked: "Processing needs attention",
              cancelled: "Session ended",
            } as Record<string, string>
          )[item.status] ?? "Your approval is needed");

  return (
    <article className={`message-card ${actionable ? "needs-approval" : ""}`}>
      <div className="message-topline">
        <span className="eyebrow">{item.session_title}</span>
        <span className="message-stage">
          Step {item.step ?? stage.step} of 5
        </span>
      </div>
      {item.auction_id && (
        <Link className="text-link" to={`/market/${item.auction_id}`}>
          View auction
        </Link>
      )}
      <div className="message-title">
        <span className="message-symbol" aria-hidden="true">
          {actionable ? <ShieldCheck size={22} /> : <CheckCheck size={22} />}
        </span>
        <h2>{stage.title}</h2>
      </div>
      <p>{stage.description}</p>
      <div className="message-status">
        <span className={actionable ? "status-dot" : ""} />
        {status}
      </div>
      {actionable && (
        <>
          <div className="message-time">
            <Clock3 size={17} aria-hidden="true" />
            <strong>
              {connectionError ? "Reconnecting…" : formatTime(left)}
            </strong>
            <span>
              approval time left in{" "}
              {item.auction_id ? "this auction" : "this session"}
            </span>
          </div>
          <p className="message-caption">
            {connectionError
              ? "Connection lost. Remaining time will be confirmed when you reconnect."
              : early
                ? `You can approve now. Your timer starts only after bidding closes at ${new Date(item.required_from_ms!).toLocaleString()}, while a response is needed from you.`
                : item.timer_running
                  ? "Time is shared across all your approval steps. When it runs out, you are disqualified from this session."
                  : "Timer paused while the service is waiting. Your remaining time is preserved."}
          </p>
          <div className="message-actions">
            <button
              className="button primary"
              disabled={busy || deadlineReached}
              onClick={() => void approve()}
            >
              {busy ? (
                <LoaderCircle size={17} className="spin" />
              ) : (
                <Check size={17} />
              )}{" "}
              {busy
                ? "Submitting…"
                : deadlineReached
                  ? "Checking deadline…"
                  : "Approve"}
            </button>
            {item.can_withdraw !== false && (
              <button
                className="quiet-button"
                disabled={busy}
                onClick={() => setWithdraw(true)}
              >
                Withdraw bid
              </button>
            )}
          </div>
        </>
      )}
      {failure && <ErrorNotice>{failure}</ErrorNotice>}
      {withdraw && (
        <Dialog
          busy={busy}
          title="Withdraw your bid?"
          onClose={() => {
            if (!busy) setWithdraw(false);
          }}
        >
          <p>
            You will leave <strong>{item.session_title}</strong>. Your pending
            approvals in this session will be cancelled. This cannot be undone.
          </p>
          {failure && <ErrorNotice>{failure}</ErrorNotice>}
          <div className="dialog-actions">
            <button
              className="button"
              disabled={busy}
              onClick={() => setWithdraw(false)}
            >
              Keep participating
            </button>
            <button
              className="button"
              disabled={busy}
              onClick={() => void leave()}
            >
              {busy ? "Withdrawing…" : "Withdraw bid"}
            </button>
          </div>
        </Dialog>
      )}
    </article>
  );
}
