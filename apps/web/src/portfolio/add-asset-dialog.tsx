import { ImagePlus } from "lucide-react";
import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import { ApiError, message, post, request } from "../api/client";
import type { Asset } from "../api/types";
import { Dialog } from "../components/dialog";
import { ErrorNotice } from "../components/feedback";

import { clearDraft, readDraft, saveDraft } from "./pending-asset";

export function AddAssetDialog({
  accountId,
  onClose,
  onComplete,
}: {
  accountId: string;
  onClose: () => void;
  onComplete: () => void;
}) {
  const [draft, setDraft] = useState(() => readDraft(accountId));
  const [name, setName] = useState(draft?.name ?? "");
  const [description, setDescription] = useState(draft?.description ?? "");
  const [file, setFile] = useState<File | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const displayedImage =
    preview ??
    (draft?.image ? `/api/v1/media/${encodeURIComponent(draft.image)}` : null);
  useEffect(() => {
    if (!file) {
      setPreview(null);
      return;
    }
    const url = URL.createObjectURL(file);
    setPreview(url);
    return () => URL.revokeObjectURL(url);
  }, [file]);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      let input = draft;
      if (!input) {
        if (!name.trim()) throw new Error("Enter an asset name.");
        let image: string | null = null;
        if (file) {
          if (file.size > 10 * 1024 * 1024)
            throw new Error("Choose an image under 10 MB.");
          const form = new FormData();
          form.set("file", file);
          image = (
            await request<{ image: string }>("/v1/media", {
              method: "POST",
              body: form,
            })
          ).image;
        }
        input = {
          request_id: crypto.randomUUID(),
          name: name.trim(),
          description,
          image,
        };
        saveDraft(accountId, input);
        setDraft(input);
      }
      await post<Asset>("/v1/assets", input);
      try {
        clearDraft();
      } catch {
        /* The recorded asset remains authoritative. */
      }
      onComplete();
      onClose();
    } catch (failure) {
      if (failure instanceof ApiError && [400, 413].includes(failure.status)) {
        clearDraft();
        setDraft(null);
      }
      setError(failure instanceof Error ? failure.message : message(failure));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog
      title={draft ? "Finish adding your asset" : "Add an asset"}
      onClose={onClose}
      busy={busy}
    >
      <p className="muted">
        Add an asset record to your portfolio. This does not mint an NFT or
        import an on-chain token.
      </p>
      <form className="form-stack" onSubmit={(event) => void submit(event)}>
        <label className="upload-field">
          {displayedImage ? (
            <img src={displayedImage} alt="Selected asset preview" />
          ) : (
            <ImagePlus size={28} strokeWidth={1.3} />
          )}
          <span>
            Asset image <span className="muted">(optional)</span>
          </span>
          <input
            type="file"
            accept="image/png,image/jpeg,image/webp"
            disabled={busy || Boolean(draft)}
            onChange={(event) => setFile(event.target.files?.[0] ?? null)}
          />
          <span className="field-help">PNG, JPEG or WebP · up to 10 MB</span>
        </label>
        <label>
          Asset name
          <input
            value={name}
            onChange={(event) => setName(event.target.value)}
            required
            maxLength={128}
            disabled={busy || Boolean(draft)}
            autoFocus
          />
        </label>
        <label>
          Description <span className="muted">(optional)</span>
          <textarea
            value={description}
            onChange={(event) => setDescription(event.target.value)}
            maxLength={2048}
            rows={3}
            disabled={busy || Boolean(draft)}
          />
        </label>
        {error && <ErrorNotice>{error}</ErrorNotice>}
        {draft && !busy && (
          <p className="field-help">
            Your original request is saved in this browser tab. Retrying it will
            not create a second asset record.
          </p>
        )}
        <button className="button primary" disabled={busy}>
          {busy
            ? "Saving asset…"
            : draft
              ? "Retry saving asset"
              : "Add to portfolio"}
        </button>
      </form>
    </Dialog>
  );
}
