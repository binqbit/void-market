import { ArrowRight, Clock3, Layers3, Plus } from "lucide-react";
import { useState } from "react";
import { Link } from "react-router-dom";
import { message, request } from "../api/client";
import type { Asset, Transfer } from "../api/types";
import { useRemote } from "../api/use-remote";
import { useSession } from "../auth/session";
import { AssetCard } from "../portfolio/asset-card";
import { TransferHistory } from "../portfolio/transfer-history";
import { CopyAddress } from "../components/copy-address";
import { ErrorNotice, Loading } from "../components/feedback";
import { AddAssetDialog } from "../portfolio/add-asset-dialog";
import { readPending } from "../portfolio/pending-transfer";
import type { PendingTransfer } from "../portfolio/pending-transfer";
import { TransferDialog } from "../portfolio/transfer-dialog";
import { CreateAuctionDialog } from "../auctions/create-dialog";
import { MyAuctions } from "../auctions/my-auctions";

export function PortfolioPage() {
  const { profile, loading, error, refresh } = useSession();
  if (loading) return <Loading>Loading your portfolio…</Loading>;
  if (error)
    return (
      <div className="page-width page-content">
        <ErrorNotice retry={() => void refresh()}>{error}</ErrorNotice>
      </div>
    );
  if (!profile)
    return (
      <section className="empty-state page-width signed-out">
        <span className="empty-icon">
          <Layers3 size={32} />
        </span>
        <span className="eyebrow">A SPACE THAT’S YOURS</span>
        <h1>Your portfolio starts here.</h1>
        <p>
          Sign in to manage assets, receive transfers and see your ownership
          history.
        </p>
        <Link className="button primary" to="/login">
          Sign in to continue <ArrowRight size={17} />
        </Link>
      </section>
    );
  return (
    <Portfolio
      key={profile.address}
      accountId={profile.address}
      name={profile.username ? `@${profile.username}` : profile.name}
    />
  );
}

function Portfolio({ accountId, name }: { accountId: string; name: string }) {
  const [tab, setTab] = useState<"assets" | "auctions" | "history">("assets");
  const [page, setPage] = useState(0);
  const [revision, setRevision] = useState(0);
  const [adding, setAdding] = useState(false);
  const [selected, setSelected] = useState<Asset | null>(null);
  const [selling, setSelling] = useState<Asset | null>(null);
  const [pending, setPending] = useState<PendingTransfer | null>(() =>
    readPending(accountId),
  );
  const [recoveryError, setRecoveryError] = useState<string | null>(null);
  const [recovering, setRecovering] = useState(false);
  const assets = useRemote<Asset[]>(
    tab === "assets" ? `/v1/me/assets?limit=20&offset=${page * 20}` : null,
    revision,
  );
  const history = useRemote<Transfer[]>(
    tab === "history" ? `/v1/me/transfers?limit=20&offset=${page * 20}` : null,
    revision,
  );
  const active = tab === "assets" ? assets : history;

  async function resume() {
    if (!pending) return;
    setRecovering(true);
    setRecoveryError(null);
    try {
      setSelected(await request<Asset>(`/v1/assets/${pending.assetId}`));
    } catch (failure) {
      setRecoveryError(message(failure));
    } finally {
      setRecovering(false);
    }
  }

  function updated() {
    setRevision((value) => value + 1);
  }

  return (
    <div className="page-width page-content">
      <div className="page-heading">
        <div>
          <span className="eyebrow">YOUR ASSETS, IN FOCUS</span>
          <h1>My portfolio</h1>
          <p>Welcome, {name}. Everything you hold, in one place.</p>
        </div>
        <button className="button primary" onClick={() => setAdding(true)}>
          <Plus size={18} />
          Add an asset
        </button>
      </div>
      <details className="portfolio-address">
        <summary>Receive an asset</summary>
        <p className="field-help">
          Share this address with another Void Market user so they can send you
          an asset.
        </p>
        <CopyAddress address={accountId} />
      </details>
      {pending && (
        <div className="notice pending-notice">
          <Clock3 size={20} />
          <div>
            <strong>A transfer needs checking</strong>
            <p>Its outcome has not been confirmed in this browser tab.</p>
          </div>
          <button
            className="button small"
            onClick={() => void resume()}
            disabled={recovering}
          >
            {recovering ? "Opening…" : "Review transfer"}
          </button>
        </div>
      )}
      {recoveryError && <ErrorNotice>{recoveryError}</ErrorNotice>}
      <div className="portfolio-toolbar">
        <div className="tabs" role="tablist" aria-label="Portfolio view">
          <button
            role="tab"
            id="assets-tab"
            aria-selected={tab === "assets"}
            aria-controls="portfolio-panel"
            onClick={() => {
              setTab("assets");
              setPage(0);
            }}
          >
            <Layers3 size={17} />
            My assets
          </button>
          <button
            role="tab"
            id="history-tab"
            aria-selected={tab === "history"}
            aria-controls="portfolio-panel"
            onClick={() => {
              setTab("history");
              setPage(0);
            }}
          >
            <Clock3 size={17} />
            History
          </button>
          <button
            role="tab"
            id="auctions-tab"
            aria-selected={tab === "auctions"}
            aria-controls="portfolio-panel"
            onClick={() => {
              setTab("auctions");
              setPage(0);
            }}
          >
            My auctions
          </button>
        </div>
        <span className="field-help">
          {tab === "assets"
            ? "Current ownership"
            : "Internal ownership transfers"}
        </span>
      </div>
      <section
        id="portfolio-panel"
        role="tabpanel"
        aria-labelledby={`${tab}-tab`}
      >
        {tab === "auctions" ? (
          <MyAuctions />
        ) : active.loading ? (
          <Loading>
            {tab === "assets" ? "Loading assets…" : "Loading history…"}
          </Loading>
        ) : active.error ? (
          <ErrorNotice retry={active.retry}>{active.error}</ErrorNotice>
        ) : active.data?.length === 0 ? (
          <div className="empty-state panel">
            <span className="empty-icon">
              {tab === "assets" ? <Layers3 size={30} /> : <Clock3 size={30} />}
            </span>
            <h2>
              {tab === "assets"
                ? page === 0
                  ? "A little room for possibility."
                  : "No more assets."
                : page === 0
                  ? "Your story starts with a transfer."
                  : "No more transfers."}
            </h2>
            <p>
              {tab === "assets"
                ? "Assets you add or receive appear here."
                : "Sent and received assets appear here once a transfer is recorded."}
            </p>
            {tab === "assets" && page === 0 && (
              <button className="button" onClick={() => setAdding(true)}>
                <Plus size={17} />
                Add your first asset
              </button>
            )}
          </div>
        ) : tab === "assets" ? (
          <div className="asset-grid">
            {assets.data?.map((asset) => (
              <AssetCard
                key={asset.id}
                asset={asset}
                disabled={Boolean(pending)}
                onTransfer={() => setSelected(asset)}
                onSell={() => setSelling(asset)}
              />
            ))}
          </div>
        ) : (
          <TransferHistory
            transfers={history.data ?? []}
            accountId={accountId}
          />
        )}
        {tab !== "auctions" &&
          !active.loading &&
          !active.error &&
          (page > 0 || active.data?.length === 20) && (
            <div className="pagination">
              <button
                className="button small"
                disabled={page === 0}
                onClick={() => setPage(page - 1)}
              >
                Previous
              </button>
              <span>Page {page + 1}</span>
              <button
                className="button small"
                disabled={active.data?.length !== 20 || page >= 500}
                onClick={() => setPage(page + 1)}
              >
                Next
              </button>
            </div>
          )}
      </section>
      {adding && (
        <AddAssetDialog
          accountId={accountId}
          onClose={() => setAdding(false)}
          onComplete={updated}
        />
      )}
      {selected && (
        <TransferDialog
          asset={selected}
          accountId={accountId}
          pending={pending?.assetId === selected.id ? pending : null}
          onClose={() => setSelected(null)}
          onComplete={updated}
          onPending={() => setPending(readPending(accountId))}
        />
      )}
      {selling && (
        <CreateAuctionDialog asset={selling} onClose={() => setSelling(null)} />
      )}
    </div>
  );
}
