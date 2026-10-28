import { useCallback, useEffect, useState } from "react";
import { message, request } from "./client";

export function useRemote<T>(path: string | null, revision = 0) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(Boolean(path));
  const [attempt, setAttempt] = useState(0);
  const retry = useCallback(() => setAttempt((value) => value + 1), []);

  useEffect(() => {
    const controller = new AbortController();
    setData(null);
    setError(null);
    setLoading(Boolean(path));
    if (!path) return () => controller.abort();

    request<T>(path, { signal: controller.signal })
      .then(setData)
      .catch((failure: unknown) => {
        if (!controller.signal.aborted) setError(message(failure));
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });

    return () => controller.abort();
  }, [path, revision, attempt]);

  return { data, error, loading, retry };
}
