import { formatZec } from "../amounts";
import { Coins, FlaskConical, ImagePlus, Network } from "lucide-react";
import { useState } from "react";
import { Navigate } from "react-router-dom";
import { post, message } from "../api/client";
import type { Asset } from "../api/types";
import { useSession } from "../auth/session";
import { useAuctionQuery } from "../auctions/use-auctions";
import { ErrorNotice, Loading } from "../components/feedback";
import { useDemoConfig } from "../demo/config";
import { DemoGuide } from "../demo/guide";
import { DemoSwitcher } from "../demo/switcher";
import { TransactionCard } from "../demo/transaction-card";
import type { DemoStatus } from "../demo/types";
import { useMessages } from "../messages/provider";

export function DemoPage() {
  const configuration = useDemoConfig();
  const demo = configuration.enabled;
  const session = useSession();
  const query = useAuctionQuery<DemoStatus>(demo ? "/v1/demo" : null);
  const assets = useAuctionQuery<Asset[]>(
    session.profile && demo ? "/v1/me/assets?limit=100" : null,
  );
  const { snapshot, refresh } = useMessages();
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [requests, setRequests] = useState<Record<string, string>>({});
  if (configuration.loading)
    return <Loading>Loading demo configuration…</Loading>;
  if (!demo) return <Navigate to="/" replace />;

  async function claim(kind: "zec" | "nft") {
    const id = requests[kind] ?? crypto.randomUUID();
    setRequests((existing) => ({ ...existing, [kind]: id }));
    setBusy(kind);
    setError(null);
    try {
      await post(`/v1/demo/${kind}`, { request_id: id });
      setRequests((existing) => {
        const next = { ...existing };
        delete next[kind];
        return next;
      });
      await refresh();
      query.refresh();
      assets.refresh();
    } catch (failure) {
      setError(message(failure));
    } finally {
      setBusy(null);
    }
  }

  const transactions = snapshot?.transactions ?? [];
  const pending = transactions.some(
    (item) =>
      ["airdrop", "mint"].includes(item.kind) &&
      !["confirmed", "cancelled", "failed"].includes(item.status),
  );
  return (
    <div className="page-width page-content demo-page">
      <div className="page-heading">
        <div>
          <span className="demo-badge">
            <FlaskConical size={14} /> DEMO · LOCALNET
          </span>
          <h1>Try the whole exchange.</h1>
          <p>
            Three disposable accounts, real local transactions, no Google
            sign-in required.
          </p>
        </div>
      </div>
      <div className="demo-notice">
        <Network size={20} />
        <p>
          This page is available only in Demo mode. ETH, ZEC and NFTs here exist
          on local test networks and have no real monetary value.
        </p>
      </div>
      <section className="panel demo-account">
        <h2>Choose an account</h2>
        <DemoSwitcher current={query.data?.own?.alias} />
        {query.loading ? (
          <Loading>Connecting to local networks…</Loading>
        ) : query.error ? (
          <ErrorNotice retry={query.refresh}>{query.error}</ErrorNotice>
        ) : (
          query.data?.own && (
            <div className="demo-resources">
              <div>
                <span className="eyebrow">Spendable Zcash</span>
                <strong className="demo-balance">
                  {query.data.own.balance === null
                    ? "Synchronizing…"
                    : `${formatZec(query.data.own.balance)} ZEC`}
                </strong>
              </div>
              <div className="demo-resource-buttons">
                <button
                  className="button"
                  disabled={
                    busy !== null || pending || !query.data.own.wallet_ready
                  }
                  onClick={() => void claim("zec")}
                >
                  <Coins size={18} />
                  {busy === "zec" ? "Requesting…" : "Claim 10 ZEC"}
                </button>
                <button
                  className="button primary"
                  disabled={
                    busy !== null || pending || !query.data.own.wallet_ready
                  }
                  onClick={() => void claim("nft")}
                >
                  <ImagePlus size={18} />
                  {busy === "nft" ? "Requesting…" : "Get NFT"}
                </button>
              </div>
              <p className="field-help">
                Each request creates a real signed transaction. Balances and
                assets update after network confirmation.
              </p>
            </div>
          )
        )}
        {error && <ErrorNotice>{error}</ErrorNotice>}
      </section>
      <DemoGuide alias={query.data?.own?.alias} assets={assets.data ?? []} />
      {transactions.length > 0 && (
        <section>
          <h2>Your local transactions</h2>
          <div className="demo-transactions">
            {transactions.slice(0, 8).map((item) => (
              <TransactionCard key={item.id} item={item} />
            ))}
          </div>
        </section>
      )}
    </div>
  );
}
