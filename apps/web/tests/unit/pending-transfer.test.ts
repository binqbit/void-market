import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  clearPending,
  readPending,
  writePending,
} from "../../src/portfolio/pending-transfer";
import {
  clearDraft,
  readDraft,
  saveDraft,
} from "../../src/portfolio/pending-asset";

const transfer = {
  requestId: "00000000-0000-4000-8000-000000000001",
  accountId: "00000000-0000-4000-8000-000000000002",
  assetId: "00000000-0000-4000-8000-000000000003",
  recipientId: "00000000-0000-4000-8000-000000000004",
};

describe("uncertain transfer recovery", () => {
  beforeEach(() => {
    const storage = new Map<string, string>();
    vi.stubGlobal("sessionStorage", {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value),
      removeItem: (key: string) => storage.delete(key),
    });
  });

  it("retains the exact request and does not expose it to another account", () => {
    writePending(transfer);
    expect(readPending(transfer.accountId)).toEqual(transfer);
    expect(readPending(transfer.recipientId)).toBeNull();
    clearPending();
    expect(readPending(transfer.accountId)).toBeNull();
  });

  it("rejects corrupt browser data and reports failed persistence", () => {
    sessionStorage.setItem(
      "void.pending-transfer",
      JSON.stringify({ ...transfer, assetId: "invalid" }),
    );
    expect(readPending(transfer.accountId)).toBeNull();
    vi.stubGlobal("sessionStorage", {
      setItem: () => {
        throw new Error("storage unavailable");
      },
      getItem: () => {
        throw new Error("storage unavailable");
      },
    });
    expect(() => writePending(transfer)).toThrow();
    expect(readPending(transfer.accountId)).toBeNull();
  });

  it("keeps an asset registration request across reloads and isolates accounts", () => {
    const draft = {
      request_id: transfer.requestId,
      name: "Asset",
      description: "",
      image: null,
    };
    saveDraft(transfer.accountId, draft);
    expect(readDraft(transfer.accountId)).toEqual(draft);
    expect(readDraft(transfer.recipientId)).toBeNull();
    clearDraft();
    expect(readDraft(transfer.accountId)).toBeNull();
  });
});
