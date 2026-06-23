import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import SaveToPlaylistPopover from "./SaveToPlaylistPopover";

const listLibraryLists = vi.fn();
const createUserList = vi.fn();
const addUserListMember = vi.fn();

const mockApi = {
  listLibraryLists,
  createUserList,
  addUserListMember,
};

vi.mock("../../context/VaultContext", () => ({
  useVaultApi: () => mockApi,
}));

describe("SaveToPlaylistPopover", () => {
  beforeEach(() => {
    listLibraryLists.mockReset();
    createUserList.mockReset();
    addUserListMember.mockReset();
    listLibraryLists.mockResolvedValue([
      {
        id: "user:p1",
        list_kind: "user",
        list_type: "playlist",
        title: "Music",
        member_count: 1,
        overlay: false,
      },
      {
        id: "user:p2",
        list_kind: "user",
        list_type: "playlist",
        title: "Gaming",
        member_count: 0,
        overlay: false,
      },
    ]);
  });

  afterEach(() => {
    cleanup();
  });

  it("filters playlists by search query", async () => {
    render(
      <SaveToPlaylistPopover entityId="v1" onClose={vi.fn()} />,
    );

    expect(await screen.findByRole("option", { name: /Music/i })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Search playlists" }));
    fireEvent.change(screen.getByPlaceholderText("Search playlists"), {
      target: { value: "Gamin" },
    });

    expect(screen.queryByRole("option", { name: /Music/i })).toBeNull();
    expect(await screen.findByRole("option", { name: /Gaming/i })).toBeTruthy();
  });

  it("creates a playlist and adds the video", async () => {
    const onClose = vi.fn();
    createUserList.mockResolvedValue({ id: "new-list" });
    addUserListMember.mockResolvedValue({ status: "added" });

    render(<SaveToPlaylistPopover entityId="v1" onClose={onClose} />);

    await waitFor(() => {
      expect(screen.getByText("Create new playlist")).toBeTruthy();
    });

    fireEvent.click(screen.getByText("Create new playlist"));
    fireEvent.change(screen.getByPlaceholderText("Playlist title"), {
      target: { value: "Road trips" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));

    await waitFor(() => {
      expect(createUserList).toHaveBeenCalledWith({
        title: "Road trips",
        list_type: "playlist",
      });
      expect(addUserListMember).toHaveBeenCalledWith("new-list", "v1");
      expect(onClose).toHaveBeenCalled();
    });
  });
});
