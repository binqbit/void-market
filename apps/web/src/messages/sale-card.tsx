import { Clock3, ShieldCheck } from "lucide-react";
import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { CompleteSaleButton } from "../auctions/complete-button";
import { accountName } from "../auctions/types";
import type { SaleRequest } from "../auctions/types";
import { formatTime, remainingTime } from "./countdown";
import { useMessages } from "./provider";

export function SaleCard({ item }: { item: SaleRequest }) {
  const { refresh, error } = useMessages();
  const [elapsed, setElapsed] = useState(0);
  useEffect(() => {
    const start = performance.now();
    setElapsed(0);
    if (!item.timer_running || error) return;
    const timer = window.setInterval(
      () => setElapsed(performance.now() - start),
      250,
    );
    return () => window.clearInterval(timer);
  }, [item.remaining_ms, item.timer_running, error]);
  return (
    <article className="message-card needs-approval">
      <div className="message-content">
        <div className="message-topline">
          <span className="eyebrow">{item.title}</span>
          <span className="message-stage">Sale completion</span>
        </div>
        <div className="message-title">
          <ShieldCheck size={20} aria-hidden="true" />
          <h2>Approve the sale transfer</h2>
        </div>
        <p className="message-description">
          Buyer <strong>{accountName(item.buyer)}</strong> ·{" "}
          <strong>{item.price} ZEC</strong>
        </p>
        <div className="message-links">
          <Link className="text-link" to={`/market/${item.auction_id}`}>
            View auction
          </Link>
        </div>
      </div>
      <div className="message-controls">
        <div className="message-time">
          <Clock3 size={15} aria-hidden="true" />
          <strong>
            {error
              ? "Reconnecting…"
              : formatTime(
                  remainingTime(item.remaining_ms, item.timer_running, elapsed),
                )}
          </strong>
          <span>{item.timer_running ? "remaining" : "paused"}</span>
        </div>
        <span className="message-budget">Shared auction budget</span>
        <CompleteSaleButton
          id={item.auction_id}
          requestId={item.request_id}
          generation={item.generation}
          title={item.title}
          price={item.price}
          buyer={item.buyer}
          paymentStatus={item.payment_status}
          onComplete={() => void refresh()}
        />
      </div>
      <p className="message-caption">
        {item.payment_status === "confirmed"
          ? "Local payment is confirmed. Approve NFT delivery, or delivery proceeds automatically when your remaining time expires."
          : "Approve to transfer ownership inside Void Market. Payment remains unverified."}{" "}
        {error
          ? "Time could not be confirmed. Reconnecting to the service."
          : item.timer_running
            ? "This uses the same 10-minute budget as your earlier approvals."
            : "Timer paused while the service is waiting."}
      </p>
    </article>
  );
}
