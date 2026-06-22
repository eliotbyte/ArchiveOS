import { useState, type ReactNode } from "react";
import { Link, NavLink, useLocation } from "react-router-dom";
import VaultSelector from "./VaultSelector";

interface AppShellProps {
  children: ReactNode;
}

const NAV_ITEMS = [
  { to: "/", label: "Library", end: true },
  { to: "/playlists", label: "Collections", end: false },
  { to: "/integrations", label: "Integrations", end: false },
  { to: "/jobs", label: "Jobs", end: false },
] as const;

export default function AppShell({ children }: AppShellProps) {
  const [navOpen, setNavOpen] = useState(false);
  const location = useLocation();
  const isViewer = location.pathname.startsWith("/entities/");

  return (
    <div className={`app-layout${isViewer ? " viewer-mode" : ""}`}>
      <header className="app-topbar">
        <button
          className="nav-toggle"
          type="button"
          aria-label="Toggle navigation"
          onClick={() => setNavOpen((open) => !open)}
        >
          ☰
        </button>
        {isViewer ? (
          <Link className="brand brand-link" to="/" onClick={() => setNavOpen(false)}>
            <span className="eyebrow">ArchiveOS</span>
            <span className="brand-title">Library</span>
          </Link>
        ) : (
          <div className="brand">
            <span className="eyebrow">ArchiveOS</span>
            <span className="brand-title">Library</span>
          </div>
        )}
        <VaultSelector />
      </header>

      {navOpen ? (
        <button
          className="nav-backdrop"
          type="button"
          aria-label="Close navigation"
          onClick={() => setNavOpen(false)}
        />
      ) : null}

      <aside className={`app-sidebar${navOpen ? " open" : ""}`}>
        <nav className="sidebar-nav">
          {NAV_ITEMS.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.end}
              onClick={() => setNavOpen(false)}
            >
              {item.label}
            </NavLink>
          ))}
        </nav>
      </aside>

      <main className={`app-main${isViewer ? " viewer-main" : ""}`}>{children}</main>
    </div>
  );
}
