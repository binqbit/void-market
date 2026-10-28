import { useState } from "react";
import { post, message } from "../api/client";
import { useSession } from "../auth/session";
import { ErrorNotice } from "../components/feedback";

export function DemoSwitcher({ current }: { current?: string }) {
  const { refresh } = useSession();
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function signIn(actor: string) {
    setBusy(actor);
    setError(null);
    try {
      await post("/v1/demo/login", { actor });
      await refresh();
    } catch (failure) {
      setError(message(failure));
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="demo-switcher">
      <div className="demo-identity-buttons">
        {["alice", "bob", "daniel"].map((actor) => (
          <button
            key={actor}
            className={`button ${current === actor ? "selected" : ""}`}
            disabled={busy !== null || current === actor}
            onClick={() => void signIn(actor)}
          >
            {busy === actor
              ? "Signing in…"
              : `Continue as ${actor[0].toUpperCase()}${actor.slice(1)}`}
          </button>
        ))}
      </div>
      {error && <ErrorNotice>{error}</ErrorNotice>}
    </div>
  );
}
