export interface PendingTransfer {
  requestId: string;
  accountId: string;
  assetId: string;
  recipientId: string;
}

const key = "void.pending-transfer";
const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function readPending(accountId: string): PendingTransfer | null {
  try {
    const value: unknown = JSON.parse(sessionStorage.getItem(key) ?? "null");
    if (!value || typeof value !== "object") return null;
    const candidate = value as Record<string, unknown>;
    for (const name of ["requestId", "accountId", "assetId", "recipientId"]) {
      if (typeof candidate[name] !== "string" || !uuid.test(candidate[name]))
        return null;
    }
    return candidate.accountId === accountId
      ? (candidate as unknown as PendingTransfer)
      : null;
  } catch {
    return null;
  }
}

export function writePending(value: PendingTransfer) {
  // Persistence must succeed before submitting ownership changes.
  sessionStorage.setItem(key, JSON.stringify(value));
}

export function clearPending() {
  sessionStorage.removeItem(key);
}
