import { LockKeyhole } from "lucide-react";
import { Link, Navigate, useSearchParams } from "react-router-dom";
import { useRemote } from "../api/use-remote";
import { useSession } from "../auth/session";
import { ErrorNotice, Loading } from "../components/feedback";

export function LoginPage() {
  const { profile, loading } = useSession();
  const config = useRemote<{ google: boolean }>("/v1/auth/config");
  const [params] = useSearchParams();

  if (loading) return <Loading>Checking your session…</Loading>;
  if (profile) return <Navigate to="/portfolio" replace />;

  return (
    <div className="auth-layout page-width">
      <div className="auth-intro">
        <span className="eyebrow">YOUR NEXT MOVE STARTS HERE</span>
        <h1>
          A space
          <br />
          for <em>your assets.</em>
        </h1>
        <p>
          One account for your portfolio, ownership history and the market
          ahead.
        </p>
        <div className="auth-decoration" aria-hidden="true">
          <LockKeyhole size={60} strokeWidth={1} />
        </div>
      </div>
      <section className="auth-panel panel">
        <span className="eyebrow">VOID MARKET ACCOUNT</span>
        <h2>Make yourself at home.</h2>
        <p className="muted">
          Sign in or create an account with Google. Then choose the username
          people will know you by.
        </p>
        {params.has("error") && (
          <ErrorNotice>
            Google sign-in could not be completed. Please start again.
          </ErrorNotice>
        )}
        {config.loading ? (
          <Loading>Checking sign-in availability…</Loading>
        ) : config.error ? (
          <ErrorNotice retry={config.retry}>{config.error}</ErrorNotice>
        ) : config.data?.google ? (
          <a className="button google-button" href="/api/v1/auth/google/start">
            <img src="/icons/google.svg" width={20} height={20} alt="" />
            <span>Continue with Google</span>
          </a>
        ) : (
          <div className="notice">
            Google sign-in is being configured. Please try again later.
          </div>
        )}
        <div className="sign-in-note">
          <LockKeyhole size={18} />
          <p>
            Google confirms your identity. Your public username is separate from
            your email.
          </p>
        </div>
        <p className="fine-print">
          Learn about <Link to="/docs#account">your account</Link> and{" "}
          <Link to="/docs#privacy">privacy on Void Market</Link>.
        </p>
      </section>
    </div>
  );
}
