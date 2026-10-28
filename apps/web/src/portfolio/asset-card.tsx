import { ArrowUpRight } from "lucide-react";
import { Link } from "react-router-dom";
import type { Asset } from "../api/types";
import { AssetImage } from "../components/asset-image";

export function AssetCard({
  asset,
  disabled,
  onTransfer,
  onWithdraw,
  onSell,
}: {
  asset: Asset;
  disabled: boolean;
  onTransfer: () => void;
  onWithdraw: () => void;
  onSell: () => void;
}) {
  return (
    <article className="asset-card">
      <AssetImage image={asset.image} name={asset.name} />
      <div className="asset-card-body">
        <span className="eyebrow">IN YOUR PORTFOLIO</span>
        <h2>{asset.name}</h2>
        <p>{asset.description || "No description provided."}</p>
        <code className="asset-id" title={asset.id}>
          {asset.id}
        </code>
        {asset.active_auction_id ? (
          <Link
            className="button full-width"
            to={`/market/${asset.active_auction_id}`}
          >
            View auction <ArrowUpRight size={17} />
          </Link>
        ) : (
          <button
            className="button primary full-width"
            disabled={disabled}
            onClick={onSell}
          >
            Sell asset
          </button>
        )}
        <button
          className="button full-width"
          disabled={disabled || Boolean(asset.active_auction_id)}
          onClick={asset.nft ? onWithdraw : onTransfer}
        >
          {asset.nft ? "Withdraw" : "Transfer"} <ArrowUpRight size={17} />
        </button>
      </div>
    </article>
  );
}
