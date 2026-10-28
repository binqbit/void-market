import { createContext, useContext, useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { ApiError, message, post, request } from "../api/client";
import type { Profile } from "../api/types";

interface Session {
  profile: Profile | null;
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  signOut: () => Promise<void>;
}

const Context = createContext<Session | null>(null);

export function SessionProvider({ children }: { children: ReactNode }) {
  const [profile, setProfile] = useState<Profile | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const revision = useRef(0);

  async function refresh() {
    const current = ++revision.current;
    setLoading(true);
    setError(null);
    try {
      const next = await request<Profile>("/v1/me");
      if (revision.current === current) setProfile(next);
    } catch (failure) {
      if (revision.current !== current) return;
      setProfile(null);
      if (!(failure instanceof ApiError && failure.status === 401)) {
        setError(message(failure));
      }
    } finally {
      if (revision.current === current) setLoading(false);
    }
  }

  async function signOut() {
    await post("/v1/auth/logout", {});
    revision.current += 1;
    setProfile(null);
    setLoading(false);
  }

  useEffect(() => {
    const expired = () => {
      revision.current += 1;
      setProfile(null);
      setError(null);
      setLoading(false);
    };
    window.addEventListener("void:session-expired", expired);
    void refresh();
    return () => {
      revision.current += 1;
      window.removeEventListener("void:session-expired", expired);
    };
  }, []);

  return (
    <Context.Provider value={{ profile, loading, error, refresh, signOut }}>
      {children}
    </Context.Provider>
  );
}

export function useSession() {
  const session = useContext(Context);
  if (!session) throw new Error("SessionProvider is missing");
  return session;
}
