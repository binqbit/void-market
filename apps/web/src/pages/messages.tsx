import { Inbox, ShieldCheck } from "lucide-react";
import { useSession } from "../auth/session";
import { ErrorNotice, Loading } from "../components/feedback";
import { MessageCard } from "../messages/message-card";
import { SessionCard } from "../messages/session-card";
import { useMessages } from "../messages/provider";
import { SaleCard } from "../messages/sale-card";
import { TransactionCard } from "../demo/transaction-card";
import { inboxEntries, needsApproval } from "../messages/inbox";
import type { InboxEntry } from "../messages/inbox";
import { SignInRedirect } from "../auth/sign-in-redirect";
import { AuctionOutcomes } from "../messages/auction-outcomes";
import type { ApprovalMessage } from "../messages/types";

export function MessagesPage() {
  const session = useSession();
  const { snapshot, loading, error, refresh } = useMessages();
  if (session.loading) return <Loading>Loading your account…</Loading>;
  if (session.error)
    return (
      <div className="page-width page-content">
        <ErrorNotice retry={() => void session.refresh()}>
          {session.error}
        </ErrorNotice>
      </div>
    );
  if (!session.profile) return <SignInRedirect />;
  const pending = snapshot?.messages.filter(needsApproval) ?? [];
  const history =
    snapshot?.messages
      .filter((item) => !pending.some((active) => active.id === item.id))
      .sort((left, right) => right.created_at_ms - left.created_at_ms) ?? [];
  const sessions =
    snapshot?.sessions.filter(
      (item) =>
        ["initializing", "active"].includes(item.status) &&
        !pending.some((message) => message.session_id === item.id),
    ) ?? [];
  const entries = inboxEntries(snapshot);
  const outcomes = snapshot?.outcomes ?? [];
  const requestGroups = new Map<string, InboxEntry[]>();
  for (const entry of entries) {
    const key = entry.item.auction_id ?? entry.key;
    const group = requestGroups.get(key) ?? [];
    group.push(entry);
    requestGroups.set(key, group);
  }
  const historyGroups = new Map<string, ApprovalMessage[]>();
  for (const item of history) {
    const key = item.auction_id ?? item.session_id;
    const group = historyGroups.get(key) ?? [];
    group.push(item);
    historyGroups.set(key, group);
  }

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
          {error
            ? "Requests unavailable"
            : snapshot
              ? `${snapshot.pending} awaiting approval`
              : "Checking requests…"}
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
      ) : !snapshot ? null : entries.length ? (
        <section className="message-list" aria-label="Awaiting your approval">
          {[...requestGroups].map(([key, group]) =>
            group.length === 1 ? (
              <InboxCard key={group[0].key} entry={group[0]} />
            ) : (
              <div className="message-group" key={key}>
                <div className="message-group-heading">
                  <h2>
                    {group[0].kind === "sale"
                      ? group[0].item.title
                      : (group[0].item.auction_title ?? "Auction requests")}
                  </h2>
                  <span>{group.length} requests</span>
                </div>
                {group.map((entry) => (
                  <InboxCard key={entry.key} entry={entry} />
                ))}
              </div>
            ),
          )}
        </section>
      ) : !error && outcomes.length === 0 ? (
        <section className="empty-state messages-empty">
          <Inbox size={36} strokeWidth={1.3} aria-hidden="true" />
          <h2>You are all caught up</h2>
          <p>New approval requests will appear here automatically.</p>
        </section>
      ) : null}
      {sessions.length > 0 && (
        <section className="message-sessions" aria-label="Your sessions">
          <h2>Your sessions</h2>
          {sessions.map((item) => (
            <SessionCard key={item.id} item={item} />
          ))}
        </section>
      )}
      <AuctionOutcomes outcomes={outcomes} />
      {history.length > 0 && (
        <section className="message-history">
          <h2>Recent activity</h2>
          {[...historyGroups].map(([key, items]) => (
            <details className="message-archive" key={key}>
              <summary>
                <strong>
                  {items[0].auction_title ?? items[0].session_title}
                </strong>
                <span>{items.length} earlier approvals</span>
              </summary>
              <div className="message-list">
                {items.map((item) => (
                  <MessageCard key={item.id} item={item} />
                ))}
              </div>
            </details>
          ))}
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
