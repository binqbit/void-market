import type { SaleRequest } from "../auctions/types";
import type { NativeTransaction } from "../demo/types";
import type { ApprovalMessage, MessageSnapshot } from "./types";

export type InboxEntry =
  | { kind: "message"; key: string; item: ApprovalMessage }
  | { kind: "sale"; key: string; item: SaleRequest }
  | { kind: "transaction"; key: string; item: NativeTransaction };

export function needsApproval(item: ApprovalMessage): boolean {
  return (
    item.status === "ready" &&
    item.participant_status === "active" &&
    item.session_status === "active"
  );
}

export function inboxEntries(snapshot: MessageSnapshot | null): InboxEntry[] {
  if (!snapshot) return [];

  const entries: InboxEntry[] = [];

  for (const item of snapshot.messages.filter(needsApproval)) {
    entries.push({ kind: "message", key: `message:${item.id}`, item });
  }

  for (const item of snapshot.sales ?? []) {
    entries.push({ kind: "sale", key: `sale:${item.request_id}`, item });
  }

  for (const item of snapshot.transactions ?? []) {
    if (!["confirmed", "cancelled"].includes(item.status)) {
      entries.push({
        kind: "transaction",
        key: `transaction:${item.id}`,
        item,
      });
    }
  }

  // All request types share the server's monotonic activity revision.
  return entries.sort(
    (left, right) =>
      right.item.revision - left.item.revision ||
      left.key.localeCompare(right.key),
  );
}
