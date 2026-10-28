import { Inbox, ShieldCheck } from "lucide-react";
import { Navigate } from "react-router-dom";
import { useSession } from "../auth/session";
import { ErrorNotice, Loading } from "../components/feedback";
import { MessageCard } from "../messages/message-card";
import { SessionCard } from "../messages/session-card";
import { useMessages } from "../messages/provider";
import { SaleCard } from "../messages/sale-card";
import { TransactionCard } from "../demo/transaction-card";
import { inboxEntries, needsApproval } from "../messages/inbox";
import type { InboxEntry } from "../messages/inbox";

export function MessagesPage() {
  const session = useSession();
  const { snapshot, loading, error, refresh } = useMessages();
  if (session.loading) return <Loading>Loading your account…</Loading>;
  if (!session.profile) return <Navigate to="/login" replace />;
  const pending = snapshot?.messages.filter(needsApproval) ?? [];
  const history =
    snapshot?.messages
      .filter((item) => !pending.some((active) => active.id === item.id))
      .sort((left, right) => right.revision - left.revision) ?? [];
  const sessions =
    snapshot?.sessions.filter(
      (item) => !pending.some((message) => message.session_id === item.id),
    ) ?? [];
  const entries = inboxEntries(snapshot);

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
      ) : entries.length ? (
        <section className="message-list" aria-label="Awaiting your approval">
          {entries.map((entry) => (
            <InboxCard key={entry.key} entry={entry} />
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

function InboxCard({ entry }: { entry: InboxEntry }) {
  switch (entry.kind) {
    case "message":
      return <MessageCard item={entry.item} />;
    case "sale":
      return <SaleCard item={entry.item} />;
    case "transaction":
      return <TransactionCard item={entry.item} />;
  }
}
