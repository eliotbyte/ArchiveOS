import { act, renderHook, waitFor } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router-dom";
import { describe, expect, it } from "vitest";
import type { ReactNode } from "react";
import { VideoPlayerProvider, useVideoPlayer } from "./VideoPlayerContext";

let router: ReturnType<typeof createMemoryRouter>;

function wrapper(initialPath: string) {
  return function ProviderWrapper({ children }: { children: ReactNode }) {
    router = createMemoryRouter(
      [{ path: "*", element: <VideoPlayerProvider>{children}</VideoPlayerProvider> }],
      { initialEntries: [initialPath] },
    );
    return <RouterProvider router={router} />;
  };
}

const sampleEntity = {
  id: "entity-1",
  title: "Test Video",
  status: "present" as const,
  tags: [],
  primary_asset_id: "asset-1",
  primary_asset_status: "present" as const,
};

describe("VideoPlayerContext mini mode", () => {
  it("keeps mini mode when paused off entity route", async () => {
    const { result } = renderHook(() => useVideoPlayer(), {
      wrapper: wrapper("/entities/entity-1"),
    });

    act(() => {
      result.current.playEntity(sampleEntity);
    });

    await waitFor(() => {
      expect(result.current.mode).toBe("full");
    });

    act(() => {
      result.current.collapseToMini();
    });

    await waitFor(() => {
      expect(result.current.mode).toBe("mini");
    });

    act(() => {
      result.current.setPlaying(false);
    });

    await waitFor(() => {
      expect(result.current.mode).toBe("mini");
      expect(result.current.currentItem).not.toBeNull();
    });
  });

  it("closes player when leaving entity route paused via back navigation", async () => {
    const { result } = renderHook(() => useVideoPlayer(), {
      wrapper: wrapper("/library/youtube"),
    });

    act(() => {
      result.current.playEntity(sampleEntity);
    });

    await waitFor(() => {
      expect(result.current.mode).toBe("full");
      expect(router.state.location.pathname).toBe("/entities/entity-1");
    });

    act(() => {
      result.current.setPlaying(false);
    });

    act(() => {
      void router.navigate("/library/youtube");
    });

    await waitFor(() => {
      expect(result.current.mode).toBe("hidden");
      expect(result.current.currentItem).toBeNull();
    });
  });

  it("enters mini mode on collapse even when already paused", async () => {
    const { result } = renderHook(() => useVideoPlayer(), {
      wrapper: wrapper("/entities/entity-1"),
    });

    act(() => {
      result.current.playEntity(sampleEntity);
      result.current.setPlaying(false);
    });

    act(() => {
      result.current.collapseToMini();
    });

    await waitFor(() => {
      expect(result.current.mode).toBe("mini");
    });
  });

  it("openFull navigates back to entity route from mini", async () => {
    const { result } = renderHook(() => useVideoPlayer(), {
      wrapper: wrapper("/library/youtube"),
    });

    act(() => {
      result.current.playEntity(sampleEntity);
    });

    await waitFor(() => {
      expect(result.current.mode).toBe("full");
    });

    act(() => {
      result.current.collapseToMini();
    });

    await waitFor(() => {
      expect(result.current.mode).toBe("mini");
      expect(router.state.location.pathname).toBe("/library/youtube");
    });

    act(() => {
      result.current.openFull();
    });

    await waitFor(() => {
      expect(result.current.mode).toBe("full");
      expect(router.state.location.pathname).toBe("/entities/entity-1");
    });
  });

  it("hides player only on explicit close", async () => {
    const { result } = renderHook(() => useVideoPlayer(), {
      wrapper: wrapper("/library/youtube"),
    });

    act(() => {
      result.current.playEntity(sampleEntity);
    });

    await waitFor(() => {
      expect(result.current.mode).toBe("full");
    });

    act(() => {
      result.current.savePlaybackPosition("asset-1", 42);
    });

    act(() => {
      result.current.collapseToMini();
    });

    await waitFor(() => {
      expect(result.current.mode).toBe("mini");
    });

    act(() => {
      result.current.closePlayer();
    });

    await waitFor(() => {
      expect(result.current.mode).toBe("hidden");
      expect(result.current.currentItem).toBeNull();
      expect(result.current.getPlaybackPosition("asset-1")).toBe(0);
    });
  });

  it("starts closed video from the beginning on replay", async () => {
    const { result } = renderHook(() => useVideoPlayer(), {
      wrapper: wrapper("/library/youtube"),
    });

    act(() => {
      result.current.playEntity(sampleEntity);
      result.current.savePlaybackPosition("asset-1", 42);
      result.current.closePlayer();
    });

    act(() => {
      result.current.playEntity(sampleEntity);
    });

    await waitFor(() => {
      expect(result.current.getPlaybackPosition("asset-1")).toBe(0);
    });
  });

  const playlistMembers = [
    {
      id: "entity-1",
      title: "First",
      status: "present" as const,
      position: 0,
      primary_asset_id: "asset-1",
      primary_asset_status: "present" as const,
    },
    {
      id: "entity-2",
      title: "Second",
      status: "present" as const,
      position: 1,
      primary_asset_id: "asset-2",
      primary_asset_status: "present" as const,
    },
  ];

  it("advances playlist in mini mode without navigating to entity page", async () => {
    const { result } = renderHook(() => useVideoPlayer(), {
      wrapper: wrapper("/library/youtube"),
    });

    act(() => {
      result.current.playQueue(playlistMembers, "entity-1");
    });

    await waitFor(() => {
      expect(result.current.mode).toBe("full");
      expect(router.state.location.pathname).toBe("/entities/entity-1");
    });

    act(() => {
      result.current.collapseToMini();
    });

    await waitFor(() => {
      expect(result.current.mode).toBe("mini");
      expect(router.state.location.pathname).toBe("/library/youtube");
    });

    act(() => {
      result.current.playNext();
    });

    await waitFor(() => {
      expect(result.current.mode).toBe("mini");
      expect(router.state.location.pathname).toBe("/library/youtube");
      expect(result.current.currentItem?.entityId).toBe("entity-2");
    });
  });

  it("goes to previous track in mini mode without navigating to entity page", async () => {
    const { result } = renderHook(() => useVideoPlayer(), {
      wrapper: wrapper("/library/youtube"),
    });

    act(() => {
      result.current.playQueue(playlistMembers, "entity-2");
    });

    await waitFor(() => {
      expect(result.current.currentItem?.entityId).toBe("entity-2");
    });

    act(() => {
      result.current.collapseToMini();
    });

    await waitFor(() => {
      expect(result.current.mode).toBe("mini");
    });

    act(() => {
      result.current.playPrevious();
    });

    await waitFor(() => {
      expect(result.current.mode).toBe("mini");
      expect(router.state.location.pathname).toBe("/library/youtube");
      expect(result.current.currentItem?.entityId).toBe("entity-1");
    });
  });
});
