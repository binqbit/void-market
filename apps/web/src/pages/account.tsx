import { LogOut, ShieldCheck } from "lucide-react";
import { useState } from "react";
import type { FormEvent } from "react";
import { Navigate, useNavigate, useSearchParams } from "react-router-dom";
import { message, post } from "../api/client";
import { useSession } from "../auth/session";
import { ErrorNotice, Loading } from "../components/feedback";
import { loginHref, safeReturnTo } from "../auth/return-to";
import { BiddingOverview } from "../account/bidding-overview";

export function AccountPage() {
  const { profile, loading, refresh, signOut } = useSession();
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const [notice, setNotice] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  if (loading) return <Loading>Loading your account…</Loading>;
  if (!profile) return <Navigate to={loginHref("/account")} replace />;

  const displayName =
    !profile.username && profile.name === "New member" ? "" : profile.name;

  async function update(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const fields = new FormData(event.currentTarget);
    setBusy(true);
    setError(null);
    setNotice("");
    try {
      await post("/v1/me/profile", {
        name: fields.get("name"),
        username: fields.get("username"),
      });
      await refresh();
      if (params.has("next")) {
        navigate(safeReturnTo(params.get("next")), { replace: true });
        return;
      }
      setNotice("Your profile has been saved.");
    } catch (failure) {
      setError(message(failure));
    } finally {
      setBusy(false);
    }
  }

  async function logout() {
    setBusy(true);
    try {
      await signOut();
      navigate("/");
    } catch (failure) {
      setError(message(failure));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="page-width page-content account-page">
      <div className="page-heading">
        <div>
          <span className="eyebrow">MAKE YOURSELF AT HOME</span>
          <h1>{profile.username ? "Your account" : "Choose your identity"}</h1>
          <p>
            {profile.username
              ? "Manage your public profile and account."
              : "Reserve a username before making your first move."}
          </p>
        </div>
        <ShieldCheck size={36} strokeWidth={1.2} />
      </div>
      {error && <ErrorNotice>{error}</ErrorNotice>}
      {notice && (
        <div className="notice" role="status">
          {notice}
        </div>
      )}
      {profile.username && <BiddingOverview key={profile.address} />}
      <div className="account-grid">
        <section className="panel">
          <h2>Public profile</h2>
          <p className="muted">
            This is what people see when they transfer an asset to you.
          </p>
          <form className="form-stack" onSubmit={(event) => void update(event)}>
            <label>
              Display name
              <input
                name="name"
                required
                maxLength={64}
                defaultValue={displayName}
                placeholder="My name"
                disabled={busy}
                autoComplete="nickname"
              />
            </label>
            <label>
              Username
              <input
                name="username"
                required
                minLength={3}
                maxLength={24}
                pattern="[a-z][a-z0-9_]{2,23}"
                defaultValue={profile.username ?? ""}
                placeholder="my_username"
                readOnly={Boolean(profile.username)}
                disabled={busy}
                autoCapitalize="none"
                spellCheck={false}
              />
              <span className="field-help">
                3–24 lowercase letters, numbers or underscores. Start with a
                letter. Your username is reserved once and cannot be changed.
              </span>
            </label>
            <button className="button primary" disabled={busy}>
              {busy ? "Saving…" : "Save profile"}
            </button>
          </form>
        </section>
        <section className="panel account-access">
          <h2>Account access</h2>
          <p className="muted">
            {profile.is_demo
              ? "Signed in with a local demo account."
              : profile.email
                ? "Signed in with Google."
                : "Signed in to your account."}
          </p>
          {profile.email && (
            <>
              <p className="account-email">{profile.email}</p>
              <p className="field-help">
                Your email is only shown to you, not on your public profile.
              </p>
            </>
          )}
          <button
            className="button quiet"
            onClick={() => void logout()}
            disabled={busy}
          >
            <LogOut size={17} />
            Sign out
          </button>
        </section>
      </div>
    </div>
  );
}
