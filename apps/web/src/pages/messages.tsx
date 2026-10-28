import { Inbox, ShieldCheck } from "lucide-react";
import { Navigate } from "react-router-dom";
import { useSession } from "../auth/session";
import { ErrorNotice, Loading } from "../components/feedback";
import { MessageCard } from "../messages/message-card";
import { SessionCard } from "../messages/session-card";
import { useMessages } from "../messages/provider";
import { SaleCard } from "../messages/sale-card";
import { TransactionCard } from "../demo/transaction-card";

export function MessagesPage() {
  const session = useSession();
  const { snapshot, loading, error, refresh } = useMessages();
  if (session.loading) return <Loading>Loading your account…</Loading>;
  if (!session.profile) return <Navigate to="/login" replace />;
  const pending =
    snapshot?.messages.filter(
      (item) =>
        item.status === "ready" &&
        item.participant_status === "active" &&
        item.session_status === "active",
    ) ?? [];
  const history =
    snapshot?.messages.filter(
      (item) => !pending.some((active) => active.id === item.id),
    ) ?? [];
  const sessions =
    snapshot?.sessions.filter(
      (item) => !pending.some((message) => message.session_id === item.id),
    ) ?? [];
  const sales = snapshot?.sales ?? [];
  const native =
    snapshot?.transactions?.filter(
      (item) => !["confirmed", "cancelled"].includes(item.status),
    ) ?? [];

  return (
    <div className="page-width page-content messages-page">
      <div className="page-heading">
        <div>
          <span className="eyebrow">YOUR NEXT STEP</span>
          <h1>Messages</h1>
          <p>
            Review requests from the arbiter and approve each step when you are
            ready.
          </p>
        </div>
        <span className="message-count">
          {snapshot?.pending ?? 0} awaiting approval
        </span>
      </div>
      {snapshot?.custody_mode === "development" && (
        <div className="custody-note">
          <ShieldCheck size={17} aria-hidden="true" />
          <span>
            Development wallet · Void Market stores your keys and signs only the
            action you approve.
          </span>
        </div>
      )}
      {error && <ErrorNotice retry={() => void refresh()}>{error}</ErrorNotice>}
      {loading && !snapshot ? (
        <Loading>Loading messages…</Loading>
      ) : pending.length || sales.length || native.length ? (
        <section className="message-list" aria-label="Awaiting your approval">
          {native.map((item) => (
            <TransactionCard key={item.id} item={item} />
          ))}
          {sales.map((item) => (
            <SaleCard key={item.auction_id} item={item} />
          ))}
          {pending.map((item) => (
            <MessageCard key={item.id} item={item} />
          ))}
        </section>
      ) : (
        <section className="empty-state messages-empty">
          <Inbox size={36} strokeWidth={1.3} aria-hidden="true" />
          <h2>You are all caught up</h2>
          <p>New approval requests will appear here automatically.</p>
        </section>
      )}
      {sessions.length > 0 && (
        <section className="message-sessions" aria-label="Your sessions">
          <h2>Your sessions</h2>
          {sessions.map((item) => (
            <SessionCard key={item.id} item={item} />
          ))}
        </section>
      )}
      {history.length > 0 && (
        <section className="message-history">
          <h2>Recent activity</h2>
          <div className="message-list">
            {history.map((item) => (
              <MessageCard key={item.id} item={item} />
            ))}
          </div>
        </section>
      )}
    </div>
  );
}
