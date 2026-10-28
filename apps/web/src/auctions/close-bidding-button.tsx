import { TimerOff } from "lucide-react";
import { useState } from "react";
import { message, post } from "../api/client";
import { Dialog } from "../components/dialog";
import { ErrorNotice } from "../components/feedback";
import { useMessages } from "../messages/provider";

export function CloseBiddingButton({
  id,
  activeBuyers,
  onComplete,
}: {
  id: string;
  activeBuyers: number;
  onComplete: () => void;
}) {
  const { refresh } = useMessages();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function closeBidding() {
    setBusy(true);
    setError(null);
    try {
      await post(`/v1/auctions/${id}/close-bidding`, {});
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
      <button
        className="button demo-close-bidding"
        onClick={() => setOpen(true)}
      >
        <TimerOff size={17} /> Close bidding now
      </button>
      {open && (
        <Dialog
          title="Close bidding now?"
          busy={busy}
          onClose={() => setOpen(false)}
        >
          <p>
            This moves the auction end to one hour from now, closing new bids
            immediately. Approvals already collected are kept; everyone must
            finish their remaining requests in Messages. Your shared 10-minute
            response budget now applies only while you have a request to
            approve.
          </p>
          <p>
            Once the checks finish, the auction determines its result
            automatically. Payment and NFT delivery still require their normal
            approvals.
          </p>
          {activeBuyers === 0 && (
            <p className="notice">
              There are no active buyers. Closing now will end without a sale.
            </p>
          )}
          {error && <ErrorNotice>{error}</ErrorNotice>}
          <div className="button-row">
            <button
              className="button"
              disabled={busy}
              onClick={() => setOpen(false)}
            >
              Keep bidding open
            </button>
            <button
              className="button demo-close-bidding"
              disabled={busy}
              onClick={() => void closeBidding()}
            >
              {busy ? "Closing…" : "Close bidding now"}
            </button>
          </div>
        </Dialog>
      )}
    </>
  );
}
