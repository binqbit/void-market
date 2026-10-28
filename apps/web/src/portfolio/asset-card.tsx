import { ArrowUpRight } from "lucide-react";
import type { Asset } from "../api/types";
import { AssetImage } from "../components/asset-image";

export function AssetCard({
  asset,
  disabled,
  onTransfer,
}: {
  asset: Asset;
  disabled: boolean;
  onTransfer: () => void;
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
        <button
          className="button full-width"
          disabled={disabled}
          onClick={onTransfer}
        >
          Transfer <ArrowUpRight size={17} />
        </button>
      </div>
    </article>
  );
}
