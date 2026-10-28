import { Link } from "react-router-dom";

export function Brand() {
  return (
    <Link className="brand" to="/" aria-label="Void Market home">
      <span className="brand-emblem" aria-hidden="true">
        <img
          className="brand-mark"
          src="/brand-mark.svg"
          width="60"
          height="60"
          alt=""
        />
      </span>
      <span className="brand-wordmark" aria-hidden="true">
        <span className="brand-name">VOID</span>
        <span className="brand-market">MARKET</span>
      </span>
    </Link>
  );
}
