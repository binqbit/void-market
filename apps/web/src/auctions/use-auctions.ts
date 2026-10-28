import { useCallback, useEffect, useState } from "react";
import { message, request } from "../api/client";
import { useSession } from "../auth/session";

export function useAuctionQuery<T>(path: string | null) {
  const { profile } = useSession();
  const key = `${profile?.address ?? "anonymous"}:${path}`;
  const [attempt, setAttempt] = useState(0);
  const [state, setState] = useState<{
    key: string;
    data: T | null;
    error: string | null;
  } | null>(null);
  const refresh = useCallback(() => setAttempt((value) => value + 1), []);

  useEffect(() => {
    const controller = new AbortController();
    let timer: number | undefined;
    async function load() {
      if (!path) return;
      try {
        const data = await request<T>(path, { signal: controller.signal });
        if (!controller.signal.aborted) setState({ key, data, error: null });
      } catch (error) {
        if (!controller.signal.aborted)
          setState((previous) => ({
            key,
            data: previous?.key === key ? previous.data : null,
            error: message(error),
          }));
      } finally {
        if (!controller.signal.aborted)
          timer = window.setTimeout(() => void load(), 5000);
      }
    }
    void load();
    return () => {
      controller.abort();
      window.clearTimeout(timer);
    };
  }, [path, key, attempt]);

  const current = state?.key === key ? state : null;
  return {
    data: current?.data ?? null,
    error: current?.error ?? null,
    loading: Boolean(path && !current),
    refresh,
  };
}
