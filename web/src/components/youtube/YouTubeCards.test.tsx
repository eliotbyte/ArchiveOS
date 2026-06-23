import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EntityListItem } from "../../api/client";
import YouTubeVideoCard from "./YouTubeCards";

const addToWatchLater = vi.fn();
const listLibraryLists = vi.fn();

const mockApi = {
  addToWatchLater,
  listLibraryLists,
  createUserList: vi.fn(),
  addUserListMember: vi.fn(),
};

vi.mock("../../context/VaultContext", () => ({
  useVault: () => ({
    assetContentUrl: (assetId: string) => `/assets/${assetId}`,
  }),
  useVaultApi: () => mockApi,
}));

const entity: EntityListItem = {
  id: "v1",
  title: "Cool Video",
  kind: "video",
  status: "present",
  source: "youtube",
  channel: "Test Channel",
  tags: [],
  primary_asset_id: "a1",
  primary_asset_status: "present",
};

describe("YouTubeVideoCard", () => {
  beforeEach(() => {
    addToWatchLater.mockReset();
    listLibraryLists.mockReset();
    listLibraryLists.mockResolvedValue([]);
  });

  afterEach(() => {
    cleanup();
  });

  it("starts playback on plain click without following the link", () => {
    const onPlay = vi.fn();
    const preventDefault = vi.spyOn(Event.prototype, "preventDefault");

    render(
      <MemoryRouter>
        <YouTubeVideoCard entity={entity} onPlay={onPlay} />
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByRole("link", { name: "Cool Video", hidden: false }));

    expect(onPlay).toHaveBeenCalledWith(entity);
    expect(preventDefault).toHaveBeenCalled();

    preventDefault.mockRestore();
  });

  it("keeps link navigation for modified clicks", () => {
    const onPlay = vi.fn();
    const preventDefault = vi.spyOn(Event.prototype, "preventDefault");

    render(
      <MemoryRouter>
        <YouTubeVideoCard entity={entity} onPlay={onPlay} />
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByRole("link", { name: "Cool Video" }), {
      ctrlKey: true,
    });

    expect(onPlay).not.toHaveBeenCalled();
    expect(preventDefault).not.toHaveBeenCalled();

    preventDefault.mockRestore();
  });

  it("opens actions menu without starting playback", async () => {
    const onPlay = vi.fn();

    render(
      <MemoryRouter>
        <YouTubeVideoCard entity={entity} onPlay={onPlay} />
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByRole("button", { name: "More actions" }));

    expect(onPlay).not.toHaveBeenCalled();
    expect(screen.getByRole("menuitem", { name: /Save to Watch Later/i })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: /Save to playlist/i })).toBeTruthy();
  });

  it("adds to watch later from the actions menu", async () => {
    addToWatchLater.mockResolvedValue({ status: "added" });

    render(
      <MemoryRouter>
        <YouTubeVideoCard entity={entity} />
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByRole("button", { name: "More actions" }));
    fireEvent.click(screen.getByRole("menuitem", { name: /Save to Watch Later/i }));

    await waitFor(() => {
      expect(addToWatchLater).toHaveBeenCalledWith("v1");
    });
  });

  it("renders channel avatar only when preview exists", () => {
    const withAvatar: EntityListItem = {
      ...entity,
      channel_entity_id: "c1",
      channel_avatar_preview: {
        entity_id: "c1",
        asset_id: "avatar-asset",
        kind: "avatar",
        preview_role: "avatar",
        status: "present",
      },
    };

    const { container } = render(
      <MemoryRouter>
        <YouTubeVideoCard entity={withAvatar} />
      </MemoryRouter>,
    );

    const avatarLink = container.querySelector(".yt-channel-avatar-link");
    expect(avatarLink?.getAttribute("href")).toBe("/library/youtube/channels/c1");
  });

  it("omits avatar column when preview is missing", () => {
    const { container } = render(
      <MemoryRouter>
        <YouTubeVideoCard entity={entity} />
      </MemoryRouter>,
    );

    expect(container.querySelector(".yt-channel-avatar-link")).toBeNull();
  });

  it("omits avatar column when preview asset is not present", () => {
    const withBrokenAvatar: EntityListItem = {
      ...entity,
      channel_entity_id: "c1",
      channel_avatar_preview: {
        entity_id: "c1",
        asset_id: "avatar-asset",
        kind: "avatar",
        preview_role: "avatar",
        status: "missing_local",
      },
    };

    const { container } = render(
      <MemoryRouter>
        <YouTubeVideoCard entity={withBrokenAvatar} />
      </MemoryRouter>,
    );

    expect(container.querySelector(".yt-channel-avatar-link")).toBeNull();
    expect(container.querySelector(".has-avatar")).toBeNull();
  });
});
