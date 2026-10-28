import {
  ArrowRight,
  LockKeyhole,
  ScanLine,
  ShieldCheck,
  Store,
} from "lucide-react";
import { Link } from "react-router-dom";

export function MarketPage() {
  return (
    <div className="page-width page-content">
      <div className="page-heading">
        <div>
          <span className="eyebrow">DISCOVER THE NEXT CHAPTER</span>
          <h1>The market</h1>
          <p>Assets in view. Bids under wraps.</p>
        </div>
        <span className="pill">
          <span className="status-dot" /> Trading in development
        </span>
      </div>
      <section className="market-feature">
        <div>
          <span className="eyebrow">PRIVATE AUCTIONS</span>
          <h2>
            A clear view of the asset.
            <br />
            <em>A private view of the price.</em>
          </h2>
          <p>
            Inspect the image, description and sale terms before deciding to
            bid. The planned auction flow compares encrypted values and reveals
            the outcome defined by its rules.
          </p>
          <Link className="text-link" to="/docs#auctions">
            Explore the auction model <ArrowRight size={17} />
          </Link>
        </div>
        <div className="market-symbol" aria-hidden="true">
          <ScanLine size={115} strokeWidth={0.65} />
          <LockKeyhole size={30} />
        </div>
      </section>
      <section className="empty-state market-empty">
        <span className="empty-icon">
          <Store size={30} strokeWidth={1.4} />
        </span>
        <span className="eyebrow">NO LIVE LISTINGS YET</span>
        <h2>The market is getting ready.</h2>
        <p>
          Listings, bids and purchases will appear here when trading is
          available. In the meantime, you can manage your assets in your
          portfolio.
        </p>
        <Link className="button primary" to="/portfolio">
          Go to my portfolio <ArrowRight size={17} />
        </Link>
      </section>
      <div className="market-details">
        <div>
          <ShieldCheck size={19} />
          <span>Sale rules will be visible before you bid.</span>
        </div>
        <div>
          <LockKeyhole size={19} />
          <span>No bids or payments are accepted on this page yet.</span>
        </div>
      </div>
    </div>
  );
}
