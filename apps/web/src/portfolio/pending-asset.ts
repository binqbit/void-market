export interface AssetDraft {
  request_id: string;
  name: string;
  description: string;
  image: string | null;
}

const key = "void.pending-asset";

export function readDraft(accountId: string): AssetDraft | null {
  try {
    const saved = JSON.parse(sessionStorage.getItem(key) ?? "null");
    if (saved?.accountId !== accountId) return null;
    const draft = saved.draft;
    if (
      typeof draft?.request_id !== "string" ||
      typeof draft?.name !== "string" ||
      typeof draft?.description !== "string" ||
      !(draft.image === null || typeof draft.image === "string")
    )
      return null;
    return draft;
  } catch {
    return null;
  }
}

export function saveDraft(accountId: string, draft: AssetDraft) {
  sessionStorage.setItem(key, JSON.stringify({ accountId, draft }));
}

export function clearDraft() {
  sessionStorage.removeItem(key);
}
