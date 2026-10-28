import {
  ArrowRight,
  ArrowUpRight,
  EyeOff,
  Fingerprint,
  Layers3,
  LockKeyhole,
  ShieldCheck,
  ShoppingBag,
  Store,
} from "lucide-react";
import { Link } from "react-router-dom";

export function HomePage() {
  return (
    <>
      <section className="hero page-width">
        <div className="hero-copy">
          <span className="eyebrow">
            <span className="status-dot" /> Built around privacy
          </span>
          <h1>
            Your assets.
            <br />
            Your next move.
            <br />
            <em>On your terms.</em>
          </h1>
          <p className="hero-lead">
            A home for your digital assets. A new approach to markets, where
            encrypted bids can be compared without putting everyone’s number on
            display.
          </p>
          <div className="button-row">
            <Link className="button primary" to="/market">
              Explore the market <ArrowUpRight size={18} />
            </Link>
            <Link className="button quiet" to="/docs">
              How it works <ArrowRight size={18} />
            </Link>
          </div>
          <div className="hero-footnote">
            <ShieldCheck size={16} /> Private comparisons, clear approvals and
            traceable asset transfers.
          </div>
        </div>
        <div
          className="protocol-art"
          aria-label="Concept: encrypted seller reserve and buyer bid are compared, revealing a match result"
        >
          <div className="art-topline">
            <span>THE PRIVATE MARKET MODEL</span>
            <span className="mono">01 — 03</span>
          </div>
          <div className="orbital orbital-one" />
          <div className="orbital orbital-two" />
          <div className="cipher-card seller">
            <span className="art-label">
              <Store size={14} aria-hidden="true" /> Seller
            </span>
            <span className="cipher-description">Offers an asset</span>
            <div className="cipher-number">••••••</div>
            <span className="cipher-meta">
              <LockKeyhole size={13} aria-hidden="true" /> Reserve price
            </span>
          </div>
          <div className="cipher-core">
            <div>
              <Fingerprint size={50} strokeWidth={1} />
            </div>
            <span>PRIVATE CHECK</span>
          </div>
          <div className="cipher-card buyer">
            <span className="art-label">
              <ShoppingBag size={14} aria-hidden="true" /> Buyer
            </span>
            <span className="cipher-description">Places a bid</span>
            <div className="cipher-number">••••••</div>
            <span className="cipher-meta">
              <LockKeyhole size={13} aria-hidden="true" /> Private bid
            </span>
          </div>
          <div className="art-result">
            <ShieldCheck size={19} />
            <div>
              <strong>A result. Not everyone’s numbers.</strong>
              <span>Encrypted input → private comparison → outcome</span>
            </div>
          </div>
          <span className="art-caption">
            Seller reserve → buyer bids → private comparison
          </span>
        </div>
      </section>
      <section
        className="principles page-width"
        aria-label="What Void Market offers"
      >
        <article>
          <span className="feature-icon">
            <Layers3 size={22} />
          </span>
          <h2>One place for your assets</h2>
          <p>
            See what you hold, add asset information and follow ownership
            transfers from your portfolio.
          </p>
        </article>
        <article>
          <span className="feature-icon">
            <EyeOff size={22} />
          </span>
          <h2>Numbers with boundaries</h2>
          <p>
            The auction model is built around encrypted comparisons, with clear
            rules for what each participant learns.
          </p>
        </article>
        <article>
          <span className="feature-icon">
            <ShieldCheck size={22} />
          </span>
          <h2>Know what happens next</h2>
          <p>
            Review the asset and recipient before a transfer. Keep a receipt
            once ownership has changed.
          </p>
        </article>
      </section>
      <section className="home-bottom page-width">
        <div>
          <span className="eyebrow">START WITH WHAT YOU OWN</span>
          <h2>Your collection, in focus.</h2>
          <p>
            Manage assets and their history in a simple, connected workspace.
          </p>
        </div>
        <Link className="button primary" to="/portfolio">
          Open my portfolio <ArrowRight size={18} />
        </Link>
      </section>
    </>
  );
}
