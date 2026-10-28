import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import type { ReactNode } from "react";
import { message, request } from "../api/client";
import { useSession } from "../auth/session";
import type { MessageSnapshot } from "./types";

interface Messages {
  snapshot: MessageSnapshot | null;
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
}

const Context = createContext<Messages | null>(null);

export function MessagesProvider({ children }: { children: ReactNode }) {
  const { profile } = useSession();
  const account = profile?.address ?? null;
  const [data, setData] = useState<{
    account: string;
    snapshot: MessageSnapshot;
  } | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const generation = useRef(0);
  const active = useRef<Promise<void> | null>(null);
  const refreshAgain = useRef(false);
  const abort = useRef<AbortController | null>(null);

  const refresh = useCallback(async () => {
    if (!account) return;
    if (active.current) {
      refreshAgain.current = true;
      return active.current;
    }
    const current = generation.current;
    const controller = new AbortController();
    abort.current = controller;
    const operation = (async () => {
      setLoading(true);
      try {
        do {
          refreshAgain.current = false;
          const snapshot = await request<MessageSnapshot>("/v1/messages", {
            signal: controller.signal,
          });
          if (generation.current === current) {
            setData({ account, snapshot });
            setError(null);
          }
        } while (refreshAgain.current && generation.current === current);
      } catch (failure) {
        if (generation.current === current && !controller.signal.aborted)
          setError(message(failure));
      } finally {
        if (generation.current === current) setLoading(false);
      }
    })();
    active.current = operation;
    await operation;
    if (active.current === operation) active.current = null;
  }, [account]);

  useEffect(() => {
    generation.current += 1;
    abort.current?.abort();
    active.current = null;
    refreshAgain.current = false;
    setData(null);
    setError(null);
    if (!account) {
      setLoading(false);
      return;
    }
    void refresh();
    let connected = false;
    const events = new EventSource("/api/v1/messages/events", {
      withCredentials: true,
    });
    events.onopen = () => {
      connected = true;
      void refresh();
    };
    events.onerror = () => {
      connected = false;
    };
    events.addEventListener("messages", () => void refresh());
    events.addEventListener("unavailable", () => {
      connected = false;
      void refresh();
    });
    const fallback = window.setInterval(() => {
      if (!connected) void refresh();
    }, 5000);
    return () => {
      generation.current += 1;
      abort.current?.abort();
      active.current = null;
      refreshAgain.current = false;
      events.close();
      window.clearInterval(fallback);
    };
  }, [account, refresh]);

  return (
    <Context.Provider
      value={{
        snapshot: data?.account === account ? data.snapshot : null,
        loading,
        error,
        refresh,
      }}
    >
      {children}
    </Context.Provider>
  );
}

export function useMessages() {
  const context = useContext(Context);
  if (!context) throw new Error("MessagesProvider is missing");
  return context;
}
