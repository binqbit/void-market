import { ArrowRight, BookOpen, Check, Eye, LockKeyhole } from "lucide-react";
import { Link } from "react-router-dom";
import { useDemoMode } from "../demo/config";

export function DocsPage() {
  const demo = useDemoMode();
  return (
    <div className="page-width page-content">
      <div className="page-heading">
        <div>
          <span className="eyebrow">HOW THE MARKET WORKS</span>
          <h1>Understand Void Market</h1>
          <p>
            Follow an asset from your portfolio through bidding and delivery.
          </p>
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
            <h2>Your next step</h2>
            <p>
              Sign in, choose your public username and open My portfolio. Use
              Market to find auctions, and Messages to review requests that need
              your approval.
            </p>
            <div className="notice">
              <Check size={19} />
              <div>
                <strong>
                  {demo ? "Demo · local networks" : "Internal asset records"}
                </strong>
                <p>
                  {demo
                    ? "Demo offers Alice, Bob and Daniel accounts, local ZEC and collectible NFTs. The transactions run on local Ethereum and Zcash networks and have no real monetary value."
                    : "Asset records and ownership transfers stay inside Void Market. An internal record is not an NFT minted on a blockchain, and internal sales do not verify payment."}
                </p>
              </div>
            </div>
            {demo && (
              <Link className="text-link" to="/demo">
                Open the guided demo <ArrowRight size={16} />
              </Link>
            )}
          </section>
          <section id="assets">
            <span className="eyebrow">02 / ASSETS & TRANSFERS</span>
            <h2>Know what you are moving</h2>
            <p>
              Your portfolio shows assets you currently own. An asset locked in
              an auction opens that auction; an available asset can be sold or
              transferred. Its image and description help identify it.
            </p>
            <ol className="steps">
              <li>
                <strong>Choose the asset.</strong>
                <span>Select Transfer on its card.</span>
              </li>
              <li>
                <strong>Check the recipient.</strong>
                <span>
                  Review the destination account and asset before approving.
                </span>
              </li>
              <li>
                <strong>Check the result.</strong>
                <span>
                  Messages shows pending actions; History shows the transaction
                  and its status.
                </span>
              </li>
            </ol>
            <p>
              A submitted transfer may still be waiting for confirmation. If a
              connection fails, check the existing request before retrying.
              Confirmed ownership changes cannot be undone by the sender.
            </p>
          </section>
          <section id="auctions">
            <span className="eyebrow">03 / PRIVATE AUCTIONS</span>
            <h2>From a private bid to a completed sale</h2>
            <div className="docs-flow">
              <span>
                <LockKeyhole size={18} /> Submit private values
              </span>
              <ArrowRight size={18} />
              <span>
                <Eye size={18} /> Compare privately
              </span>
              <ArrowRight size={18} />
              <span>
                <Check size={18} /> Confirm delivery
              </span>
            </div>
            <p>
              The seller sets a reserve and an auction end time. Bidding closes
              one hour earlier; the final hour is for approvals only. Each buyer
              can submit one fixed bid. A bid qualifies at or above the reserve;
              equal bids favor the earlier accepted bid. At most three buyers
              can join, and withdrawing does not free a place or allow reentry.
            </p>
            <p>
              Approval requests can arrive while bidding is open. You can
              approve early without spending your time budget. After bidding
              closes, each participant has one shared 10-minute budget across
              their required responses. It runs only while that person needs to
              act; waiting for other people or the service pauses it. Missing a
              required approval deadline can disqualify you.
            </p>
            <p>
              {demo
                ? "Before publication, the seller approves sending the NFT to the assigned arbiter. After the private checks select a buyer, that buyer reviews and pays the ZEC invoice. Full confirmed payment allows the seller to approve delivery; if the seller's remaining response time expires, the publication terms authorize delivery automatically."
                : "After the private checks select a buyer, the seller approves the internal ownership transfer. Payment is not processed or verified in this mode."}
            </p>
            {demo && (
              <p>
                Pay the full invoice before your response time expires. A
                partial payment needs a top-up and is not automatically
                refunded. Payment authorization fixes the selected buyer.
                Ethereum and Zcash transfers are separate, so this is a
                custodial, non-atomic exchange.
              </p>
            )}
          </section>
          <section id="privacy">
            <span className="eyebrow">04 / PRIVACY BOUNDARIES</span>
            <h2>What is visible, and to whom</h2>
            <p>
              Other buyers cannot see your bid, and buyers cannot see the
              seller's reserve. The selected buyer and seller can see the agreed
              price. Images, descriptions, public usernames and auction outcomes
              are visible information.
            </p>
            <p>
              This development version uses platform-held keys. Void Market
              receives the amounts you enter and performs approved operations
              for you. Private comparisons do not hide those inputs from the
              platform or make all activity anonymous.
            </p>
            <p>
              Do not put personal or confidential information in asset metadata.
              Network transactions also have their own visibility rules.
            </p>
          </section>
          <section id="account">
            <span className="eyebrow">05 / YOUR ACCOUNT</span>
            <h2>One account, a public identity</h2>
            <p>
              Use Google to sign in. In Demo, you can also choose one of the
              demonstration accounts. Your display name and unique username
              identify you to other users; your email is not public. A reserved
              username cannot be changed.
            </p>
            <p>
              Your browser keeps the session in an HTTP-only cookie. Signing out
              ends that session without removing your assets or history.
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
