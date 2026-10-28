import { createContext, useContext, useEffect, useState } from "react";
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

  async function refresh() {
    setLoading(true);
    setError(null);
    try {
      setProfile(await request<Profile>("/v1/me"));
    } catch (failure) {
      setProfile(null);
      if (!(failure instanceof ApiError && failure.status === 401)) {
        setError(message(failure));
      }
    } finally {
      setLoading(false);
    }
  }

  async function signOut() {
    await post("/v1/auth/logout", {});
    setProfile(null);
  }

  useEffect(() => {
    void refresh();
    const expired = () => setProfile(null);
    window.addEventListener("void:session-expired", expired);
    return () => window.removeEventListener("void:session-expired", expired);
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
