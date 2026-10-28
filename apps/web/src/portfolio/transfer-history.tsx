import { ArrowDownLeft, ArrowUpRight } from "lucide-react";
import type { Transfer } from "../api/types";

export function TransferHistory({
  transfers,
  accountId,
}: {
  transfers: Transfer[];
  accountId: string;
}) {
  return (
    <div className="history-list">
      {transfers.map((transfer) => {
        const received = transfer.recipient_id === accountId;
        const counterparty = received
          ? transfer.sender_id
          : transfer.recipient_id;
        return (
          <article className="history-item" key={transfer.id}>
            <span className="history-icon">
              {received ? (
                <ArrowDownLeft size={21} />
              ) : (
                <ArrowUpRight size={21} />
              )}
            </span>
            <div>
              <strong>{received ? "Asset received" : "Asset sent"}</strong>
              <p className="break-all">Asset {transfer.asset_id}</p>
              <span className="field-help break-all">
                {received ? "From" : "To"} {counterparty}
              </span>
              <details>
                <summary>Transfer receipt</summary>
                <code className="break-all">{transfer.id}</code>
              </details>
            </div>
            <time dateTime={transfer.created_at}>
              {new Date(transfer.created_at).toLocaleString(undefined, {
                dateStyle: "medium",
                timeStyle: "short",
              })}
            </time>
          </article>
        );
      })}
    </div>
  );
}
