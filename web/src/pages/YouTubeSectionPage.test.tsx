import { render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { VideoPlayerProvider } from "../context/VideoPlayerContext";
import YouTubeSectionPage from "./YouTubeSectionPage";

const listEntities = vi.fn();
const listCollections = vi.fn();

const mockApi = {
  listEntities,
  listCollections,
};

vi.mock("../context/VaultContext", () => ({
  useVaultApi: () => mockApi,
  useVault: () => ({
    assetContentUrl: (assetId: string) => `/assets/${assetId}`,
  }),
}));

function renderPage() {
  return render(
    <MemoryRouter>
      <VideoPlayerProvider>
        <YouTubeSectionPage />
      </VideoPlayerProvider>
    </MemoryRouter>,
  );
}

describe("YouTubeSectionPage", () => {
  beforeEach(() => {
    listEntities.mockReset();
    listCollections.mockReset();
    listEntities.mockResolvedValue([
      {
        id: "v1",
        title: "Cool Video",
        kind: "video",
        status: "present",
        source: "youtube",
        channel: "Test Channel",
        tags: [],
        primary_asset_id: "a1",
        primary_asset_status: "present",
      },
    ]);
    listCollections.mockResolvedValue([
      {
        id: "c1",
        collection_type: "youtube_playlist",
        title: "Favorites",
        member_count: 2,
      },
      {
        id: "c2",
        collection_type: "folder",
        title: "Inbox folder",
        member_count: 1,
      },
      {
        id: "c3",
        collection_type: "youtube_playlist",
        title: "Empty playlist",
        member_count: 0,
      },
    ]);
  });

  it("requests youtube videos and youtube collections with nonempty filter", async () => {
    renderPage();

    await waitFor(() => {
      expect(listEntities).toHaveBeenCalledWith({
        limit: 100,
        kind: "video",
        source: "youtube",
      });
      expect(listCollections).toHaveBeenCalledWith({ min_member_count: 1 });
    });

    expect(screen.getByRole("link", { name: /Favorites/i })).toBeTruthy();
    expect(screen.queryByText("Inbox folder")).toBeNull();
    expect(screen.queryByText("Empty playlist")).toBeNull();
    expect(screen.getByText("Cool Video")).toBeTruthy();
    expect(screen.getByText("Test Channel")).toBeTruthy();
    expect(screen.queryByText("youtube")).toBeNull();
  });
});
