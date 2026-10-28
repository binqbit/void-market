import { ArrowRight, BookOpen, Check, Eye, LockKeyhole } from "lucide-react";
import { Link } from "react-router-dom";

export function DocsPage() {
  return (
    <div className="page-width page-content">
      <div className="page-heading">
        <div>
          <span className="eyebrow">A LITTLE CLARITY GOES A LONG WAY</span>
          <h1>Understand Void Market</h1>
          <p>Your assets, your account and the idea behind private auctions.</p>
        </div>
        <BookOpen className="heading-icon" size={38} strokeWidth={1.2} />
      </div>
      <div className="docs-layout">
        <aside className="docs-nav" aria-label="On this page">
          <a href="#start">Getting started</a>
          <a href="#assets">Assets & transfers</a>
          <a href="#auctions">Private auctions</a>
          <a href="#privacy">What stays private</a>
          <a href="#account">Your account</a>
        </aside>
        <div className="docs-content">
          <section id="start">
            <span className="eyebrow">01 / GETTING STARTED</span>
            <h2>A home for your digital assets</h2>
            <p>
              Void Market brings your asset information and ownership history
              together. Start by signing in with Google, choose your username
              and open My portfolio.
            </p>
            <div className="notice">
              <Check size={19} />
              <div>
                <strong>Available today</strong>
                <p>
                  Manage asset records, attach images and transfer ownership to
                  another account within Void Market.
                </p>
              </div>
            </div>
            <p>
              The market is being built for private auctions. Listing, bidding,
              purchases, blockchain imports and payments are not available in
              this version.
            </p>
          </section>
          <section id="assets">
            <span className="eyebrow">02 / ASSETS & TRANSFERS</span>
            <h2>Know exactly what you are moving</h2>
            <p>
              Adding an asset creates a record in your portfolio with a name,
              description and optional image. It does not mint an NFT, verify
              ownership on another network or deposit money.
            </p>
            <ol className="steps">
              <li>
                <strong>Choose the asset.</strong>
                <span>
                  Open an asset in your portfolio and select Transfer.
                </span>
              </li>
              <li>
                <strong>Check the recipient.</strong>
                <span>
                  Enter their Void Market account address. Review the resolved
                  account and the exact asset before confirming.
                </span>
              </li>
              <li>
                <strong>Keep the receipt.</strong>
                <span>
                  Once recorded, the recipient becomes the owner inside Void
                  Market. Both accounts can see the transfer in History.
                </span>
              </li>
            </ol>
            <p>
              A transfer cannot be undone by the sender. If the connection is
              interrupted, review the pending transfer and check its receipt
              before retrying. Retrying that request will not create a second
              transfer.
            </p>
          </section>
          <section id="auctions">
            <span className="eyebrow">03 / THE AUCTION MODEL</span>
            <h2>Encrypted numbers. Defined outcomes.</h2>
            <p>
              The planned sealed-bid auction lets a seller define a reserve and
              buyers submit private bids. Homomorphic computation allows
              supported comparisons on encrypted values.
            </p>
            <div className="docs-flow">
              <span>
                <LockKeyhole size={18} /> Encrypt the values
              </span>
              <ArrowRight size={18} />
              <span>
                <Eye size={18} /> Compare privately
              </span>
              <ArrowRight size={18} />
              <span>
                <Check size={18} /> Reveal the outcome
              </span>
            </div>
            <p>
              A qualifying bid must be at least the reserve. Among equal bids,
              the earlier accepted bid wins. Sale terms, timing, reveal rules
              and settlement conditions must be shown before trading becomes
              available.
            </p>
            <p>
              These describe the intended auction flow. The current website does
              not run auctions or move funds.
            </p>
          </section>
          <section id="privacy">
            <span className="eyebrow">04 / PRIVACY BOUNDARIES</span>
            <h2>Privacy starts with knowing what is visible</h2>
            <p>
              Asset images, names, descriptions and public account addresses are
              visible information. Do not upload personal or confidential
              information into asset metadata.
            </p>
            <p>
              The private-auction design aims to keep bid values from being
              exposed during comparison. It does not make all activity
              anonymous: counterparties, timing, outcomes and any eventual
              blockchain settlement have their own visibility rules.
            </p>
            <p>
              Encryption does not remove every trust assumption. Auction
              integration and its final disclosure rules will be documented
              before you can place a bid.
            </p>
          </section>
          <section id="account">
            <span className="eyebrow">05 / YOUR ACCOUNT</span>
            <h2>One sign-in. Your own identity.</h2>
            <p>
              Create your account through Google, then reserve a unique
              username. Your email is private account information; your display
              name, username and receiving address form your public profile.
            </p>
            <p>
              Your browser keeps the session in an HTTP-only cookie. Signing out
              ends that session. Always use the same Google account to return to
              your portfolio.
            </p>
            <Link className="button primary" to="/portfolio">
              Open my portfolio <ArrowRight size={17} />
            </Link>
          </section>
        </div>
      </div>
    </div>
  );
}
