import { ImageOff } from "lucide-react";
import { useState } from "react";

export function AssetImage({
  image,
  name,
}: {
  image: string | null;
  name: string;
}) {
  const [failed, setFailed] = useState<string | null>(null);
  const source = image ? `/api/v1/media/${encodeURIComponent(image)}` : null;
  if (!source || failed === source) {
    return (
      <div className="asset-image image-fallback">
        <ImageOff size={30} strokeWidth={1.3} aria-hidden="true" />
        <span>Image unavailable</span>
      </div>
    );
  }
  return (
    <img
      className="asset-image"
      src={source}
      alt={name}
      loading="lazy"
      onError={() => setFailed(source)}
    />
  );
}
