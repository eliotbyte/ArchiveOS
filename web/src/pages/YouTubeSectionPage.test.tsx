import { render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { VideoPlayerProvider } from "../context/VideoPlayerContext";
import YouTubeSectionPage from "./YouTubeSectionPage";

const listEntities = vi.fn();
const listLibraryLists = vi.fn();

const mockApi = {
  listEntities,
  listLibraryLists,
};

vi.mock("../context/VaultContext", () => ({
  useVaultApi: () => mockApi,
  useVault: () => ({
    assetContentUrl: (assetId: string) => `/assets/${assetId}`,
    api: mockApi,
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
    listLibraryLists.mockReset();
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
    listLibraryLists.mockResolvedValue([
      {
        id: "smart:recently_added",
        list_kind: "smart",
        list_type: "smart_recently_added",
        title: "Recently Added",
        member_count: 1,
        overlay: true,
        icon: "🕐",
      },
      {
        id: "source:c1",
        list_kind: "source",
        list_type: "youtube_playlist",
        title: "Favorites",
        member_count: 2,
        overlay: false,
      },
      {
        id: "user:wl",
        list_kind: "user",
        list_type: "watch_later",
        title: "Watch Later",
        member_count: 0,
        overlay: true,
        icon: "⏰",
      },
    ]);
  });

  it("requests youtube videos and library lists for the section", async () => {
    renderPage();

    await waitFor(() => {
      expect(listEntities).toHaveBeenCalledWith({
        limit: 100,
        kind: "video",
        source: "youtube",
        sort: "added_desc",
      });
      expect(listLibraryLists).toHaveBeenCalledWith({ section: "youtube" });
    });

    expect(screen.getByRole("link", { name: /Recently Added/i })).toBeTruthy();
    expect(screen.getByRole("link", { name: /Favorites/i })).toBeTruthy();
    expect(screen.getByRole("link", { name: /Watch Later/i })).toBeTruthy();
    expect(screen.getByText("Cool Video")).toBeTruthy();
    expect(screen.getByText("Test Channel")).toBeTruthy();
  });
});
