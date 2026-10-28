import { Check, Copy } from "lucide-react";
import { useState } from "react";

export function CopyAddress({ address }: { address: string }) {
  const [status, setStatus] = useState("");
  async function copy() {
    try {
      await navigator.clipboard.writeText(address);
      setStatus("Copied");
    } catch {
      setStatus("Select and copy the address below");
    }
  }
  return (
    <div className="address-box">
      <code>{address}</code>
      <button
        className="icon-button"
        onClick={() => void copy()}
        aria-label="Copy account address"
      >
        {status === "Copied" ? <Check size={16} /> : <Copy size={16} />}
      </button>
      <span className="sr-only" role="status">
        {status}
      </span>
    </div>
  );
}
