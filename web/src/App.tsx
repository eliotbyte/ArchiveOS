import { Navigate, Route, Routes } from "react-router-dom";
import AppShell from "./components/AppShell";
import EntityPage from "./pages/EntityPage";
import IntegrationsPage from "./pages/IntegrationsPage";
import JobsPage from "./pages/JobsPage";
import LibraryPage from "./pages/LibraryPage";
import PlaylistsPage from "./pages/PlaylistsPage";
import YtdlpPage from "./pages/YtdlpPage";

export default function App() {
  return (
    <AppShell>
      <Routes>
        <Route path="/" element={<LibraryPage />} />
        <Route path="/entities/:id" element={<EntityPage />} />
        <Route path="/playlists" element={<PlaylistsPage />} />
        <Route path="/integrations" element={<IntegrationsPage />} />
        <Route path="/integrations/ytdlp" element={<YtdlpPage />} />
        <Route path="/jobs" element={<JobsPage />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </AppShell>
  );
}
