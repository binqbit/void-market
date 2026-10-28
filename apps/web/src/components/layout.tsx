import { CircleUserRound, Menu, ShieldCheck, X } from "lucide-react";
import { NavLink, Outlet, Link, useLocation } from "react-router-dom";
import { useLayoutEffect, useRef, useState } from "react";
import { useSession } from "../auth/session";
import { Brand } from "./brand";
import { useMessages } from "../messages/provider";
import { ThemeControl } from "./theme";
import { useDemoMode } from "../demo/config";
import { loginHref } from "../auth/return-to";

export function Layout() {
  const { profile, loading, error, refresh } = useSession();
  const location = useLocation();
  const { snapshot } = useMessages();
  const demo = useDemoMode();
  const [menuOpen, setMenuOpen] = useState(false);
  const menuButton = useRef<HTMLButtonElement>(null);
  useLayoutEffect(() => {
    setMenuOpen(false);
    const frame = requestAnimationFrame(() => {
      let target: HTMLElement | null = null;
      try {
        target = location.hash
          ? document.getElementById(decodeURIComponent(location.hash.slice(1)))
          : null;
      } catch {
        /* Invalid fragments fall back to the page start. */
      }
      if (target) {
        target.tabIndex = -1;
        target.scrollIntoView({ behavior: "instant", block: "start" });
        target.focus({ preventScroll: true });
      } else {
        window.scrollTo({ top: 0, left: 0, behavior: "instant" });
      }
    });
    return () => cancelAnimationFrame(frame);
  }, [location.pathname, location.hash]);

  return (
    <>
      <a className="skip-link" href="#main">
        Skip to content
      </a>
      <header
        className="site-header"
        onKeyDown={(event) => {
          if (event.key === "Escape" && menuOpen) {
            setMenuOpen(false);
            menuButton.current?.focus();
          }
        }}
      >
        <div className="header-inner">
          <Brand />
          <nav
            id="main-navigation"
            className={`main-nav ${menuOpen ? "is-open" : ""}`}
            aria-label="Main navigation"
            onClick={() => setMenuOpen(false)}
          >
            <NavLink to="/" end>
              <span className="nav-label">Home</span>
            </NavLink>
            <NavLink to="/market">
              <span className="nav-label">Market</span>
            </NavLink>
            <NavLink to="/portfolio">
              <span className="nav-label">My portfolio</span>
            </NavLink>
            {profile && (
              <NavLink to="/messages">
                <span className="nav-label">Messages</span>
                <span
                  className="nav-message-count"
                  aria-label={
                    snapshot?.pending
                      ? `${snapshot.pending} awaiting approval`
                      : undefined
                  }
                  aria-hidden={!snapshot?.pending}
                  style={{
                    visibility: snapshot?.pending ? "visible" : "hidden",
                  }}
                >
                  {(snapshot?.pending ?? 0) > 99
                    ? "99+"
                    : (snapshot?.pending ?? 0)}
                </span>
              </NavLink>
            )}
            <NavLink to="/docs">
              <span className="nav-label">Docs</span>
            </NavLink>
            {demo && (
              <NavLink to="/demo" className="demo-nav">
                <span className="nav-label">Demo</span>
              </NavLink>
            )}
          </nav>
          <div className="header-actions">
            <ThemeControl />
            <Link
              className="button small header-account"
              aria-label={profile ? "Your account" : "Sign in"}
              to={
                profile
                  ? "/account"
                  : loginHref(
                      location.pathname + location.search + location.hash,
                    )
              }
            >
              <CircleUserRound size={17} aria-hidden="true" />
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
            <button
              ref={menuButton}
              className="icon-button navigation-toggle"
              aria-label={
                menuOpen
                  ? "Close navigation"
                  : `Open navigation${snapshot?.pending ? `, ${snapshot.pending} approvals waiting` : ""}`
              }
              aria-controls="main-navigation"
              aria-expanded={menuOpen}
              onClick={() => {
                const opening = !menuOpen;
                setMenuOpen(opening);
                if (opening)
                  requestAnimationFrame(() =>
                    document
                      .querySelector<HTMLAnchorElement>("#main-navigation a")
                      ?.focus(),
                  );
              }}
            >
              {menuOpen ? <X size={20} /> : <Menu size={20} />}
              {Boolean(snapshot?.pending) && (
                <span className="navigation-badge" aria-hidden="true">
                  {snapshot!.pending > 99 ? "99+" : snapshot!.pending}
                </span>
              )}
            </button>
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
