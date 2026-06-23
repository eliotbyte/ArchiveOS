import { Navigate, Route, Routes } from "react-router-dom";
import AppShell from "./components/AppShell";
import VideoPlayer from "./components/VideoPlayer";
import { VideoPlayerProvider, useVideoPlayer } from "./context/VideoPlayerContext";
import EntityPage from "./pages/EntityPage";
import IntegrationsPage from "./pages/IntegrationsPage";
import JobsPage from "./pages/JobsPage";
import LibraryHomePage from "./pages/LibraryHomePage";
import LibraryListPage from "./pages/LibraryListPage";
import PlaylistsPage from "./pages/PlaylistsPage";
import SectionBrowserPage from "./pages/SectionBrowserPage";
import YouTubeSectionPage from "./pages/YouTubeSectionPage";
import YouTubeChannelPage from "./pages/YouTubeChannelPage";
import YtdlpPage from "./pages/YtdlpPage";

function GlobalVideoPlayer() {
  const { mode, currentItem, registerMiniPlayerDock, miniCorner, miniDragOffset } =
    useVideoPlayer();
  const dockStyle =
    miniDragOffset.x !== 0 || miniDragOffset.y !== 0
      ? {
          transform: `translate(${miniDragOffset.x}px, ${miniDragOffset.y}px)`,
        }
      : undefined;

  return (
    <>
      <div
        ref={registerMiniPlayerDock}
        className={`video-player-mini-dock video-player-mini-dock-${miniCorner}${mode === "mini" ? " active" : ""}`}
        style={dockStyle}
      />
      {currentItem && mode !== "hidden" ? <VideoPlayer /> : null}
    </>
  );
}

export default function App() {
  return (
    <VideoPlayerProvider>
      <AppShell>
        <Routes>
          <Route path="/" element={<LibraryHomePage />} />
          <Route path="/library/youtube" element={<YouTubeSectionPage />} />
          <Route
            path="/library/youtube/channels/:channelId"
            element={<YouTubeChannelPage />}
          />
          <Route
            path="/library/youtube/lists/:listId"
            element={<LibraryListPage />}
          />
          <Route
            path="/library/youtube/playlists/:collectionId"
            element={<LibraryListPage />}
          />
          <Route path="/library/:sectionId" element={<SectionBrowserPage />} />
          <Route path="/entities/:id" element={<EntityPage />} />
          <Route path="/playlists" element={<PlaylistsPage />} />
          <Route path="/integrations" element={<IntegrationsPage />} />
          <Route path="/integrations/ytdlp" element={<YtdlpPage />} />
          <Route path="/jobs" element={<JobsPage />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
        <GlobalVideoPlayer />
      </AppShell>
    </VideoPlayerProvider>
  );
}
