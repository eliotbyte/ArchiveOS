import { NavLink, Route, Routes } from "react-router-dom";
import { VAULT_NAME } from "./api/client";
import ArchiveForm from "./components/ArchiveForm";
import EntityPage from "./pages/EntityPage";
import ExplorePage from "./pages/ExplorePage";
import JobsPage from "./pages/JobsPage";

export default function App() {
  return (
    <div className="app-shell">
      <header className="app-header">
        <div>
          <p className="eyebrow">ArchiveOS</p>
          <h1>Media Explorer</h1>
          <p className="subtitle">Vault: {VAULT_NAME}</p>
        </div>
        <nav className="app-nav">
          <NavLink to="/" end>
            Explore
          </NavLink>
          <NavLink to="/jobs">Jobs</NavLink>
        </nav>
      </header>

      <ArchiveForm />

      <main className="app-main">
        <Routes>
          <Route path="/" element={<ExplorePage />} />
          <Route path="/entities/:id" element={<EntityPage />} />
          <Route path="/jobs" element={<JobsPage />} />
        </Routes>
      </main>
    </div>
  );
}
