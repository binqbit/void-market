import { CircleUserRound, ShieldCheck } from "lucide-react";
import { NavLink, Outlet, Link, useLocation } from "react-router-dom";
import { useLayoutEffect } from "react";
import { useSession } from "../auth/session";
import { Brand } from "./brand";
import { ThemeControl } from "./theme";

export function Layout() {
  const { profile, loading, error, refresh } = useSession();
  const location = useLocation();
  useLayoutEffect(() => {
    window.scrollTo({ top: 0, left: 0, behavior: "instant" });
  }, [location.pathname]);

  return (
    <>
      <a className="skip-link" href="#main">
        Skip to content
      </a>
      <header className="site-header">
        <div className="header-inner">
          <Brand />
          <nav className="main-nav" aria-label="Main navigation">
            <NavLink to="/" end>
              Home
            </NavLink>
            <NavLink to="/market">Market</NavLink>
            <NavLink to="/portfolio">My portfolio</NavLink>
            <NavLink to="/docs">Docs</NavLink>
          </nav>
          <div className="header-actions">
            <ThemeControl />
            <Link className="button small" to={profile ? "/account" : "/login"}>
              {profile ? (
                <CircleUserRound size={17} aria-hidden="true" />
              ) : null}
              <span className="header-account-label">
                {loading
                  ? "Account"
                  : profile
                    ? profile.username
                      ? `@${profile.username}`
                      : "My account"
                    : "Sign in"}
              </span>
            </Link>
          </div>
        </div>
      </header>
      {error && (
        <div className="connection-banner" role="alert">
          Account connection unavailable.{" "}
          <button onClick={() => void refresh()}>Retry</button>
        </div>
      )}
      <main id="main" tabIndex={-1}>
        <Outlet />
      </main>
      <footer className="site-footer">
        <Brand />
        <p>A clearer view. A more private market.</p>
        <Link to="/docs#privacy">
          <ShieldCheck size={16} aria-hidden="true" /> Understand privacy
        </Link>
      </footer>
    </>
  );
}
