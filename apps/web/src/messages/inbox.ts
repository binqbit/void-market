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
    const superseded =
      item.status === "failed" &&
      snapshot.transactions?.some(
        (next) =>
          next.id !== item.id &&
          next.kind === item.kind &&
          next.auction_id === item.auction_id &&
          next.asset_id === item.asset_id &&
          next.account_id === item.account_id &&
          next.created_at_ms > item.created_at_ms &&
          next.status !== "cancelled",
      );
    if (
      !["confirmed", "cancelled"].includes(item.status) &&
      !superseded &&
      !(
        item.status === "failed" &&
        !item.can_retry &&
        !(item.kind === "transfer" && item.direction === "outgoing")
      )
    ) {
      entries.push({
        kind: "transaction",
        key: `transaction:${item.id}`,
        item,
      });
    }
  }

  // Updates to an existing request must not move it ahead of newer requests.
  return entries.sort(
    (left, right) =>
      right.item.created_at_ms - left.item.created_at_ms ||
      left.key.localeCompare(right.key),
  );
}
