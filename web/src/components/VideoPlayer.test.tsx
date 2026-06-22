import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useEffect, useState } from "react";
import {
  createMemoryRouter,
  RouterProvider,
  useLocation,
} from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { VideoPlayerProvider, useVideoPlayer } from "../context/VideoPlayerContext";
import VideoPlayer from "./VideoPlayer";

vi.mock("../context/VaultContext", () => ({
  useVault: () => ({
    assetContentUrl: (assetId: string) => `/assets/${assetId}`,
  }),
}));

const sampleEntity = {
  id: "entity-1",
  title: "Test Video",
  status: "present" as const,
  tags: [],
  primary_asset_id: "asset-1",
  primary_asset_status: "present" as const,
};

function LocationProbe() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}</div>;
}

function MiniPlayerHarness() {
  const { registerMiniPlayerDock, playEntity, collapseToMini, mode } = useVideoPlayer();
  const [started, setStarted] = useState(false);

  useEffect(() => {
    if (!started) {
      playEntity(sampleEntity);
      setStarted(true);
    }
  }, [started, playEntity]);

  useEffect(() => {
    if (started && mode === "full") {
      collapseToMini();
    }
  }, [started, mode, collapseToMini]);

  return (
    <>
      <LocationProbe />
      <div
        ref={registerMiniPlayerDock}
        className="video-player-mini-dock video-player-mini-dock-bottom-right active"
      />
      <VideoPlayer />
    </>
  );
}

function renderMiniPlayer(initialPath = "/library/youtube") {
  const router = createMemoryRouter(
    [
      {
        path: "*",
        element: (
          <VideoPlayerProvider>
            <MiniPlayerHarness />
          </VideoPlayerProvider>
        ),
      },
    ],
    { initialEntries: [initialPath] },
  );

  return {
    router,
    ...render(<RouterProvider router={router} />),
  };
}

describe("VideoPlayer mini variant", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
    vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue(undefined);
    vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  });

  it("renders compact mini controls without full toolbar", async () => {
    renderMiniPlayer();

    await waitFor(() => {
      expect(screen.getByLabelText("Pause")).toBeTruthy();
    });

    expect(screen.getByLabelText("Close")).toBeTruthy();
    expect(screen.queryByLabelText("Settings")).toBeNull();
    expect(screen.queryByLabelText("Volume")).toBeNull();
    expect(screen.queryByLabelText("Seek")).toBeNull();
    expect(document.querySelector(".video-player-mini-progress")).toBeTruthy();
  });

  it("does not open full player after drag beyond threshold", async () => {
    const { router } = renderMiniPlayer();

    await waitFor(() => {
      expect(document.querySelector(".video-player-mini-stage")).toBeTruthy();
    });

    const stage = document.querySelector(".video-player-mini-stage")!;
    act(() => {
      fireEvent.pointerDown(stage, { clientX: 10, clientY: 10, pointerId: 1 });
      fireEvent.pointerMove(stage, { clientX: 40, clientY: 10, pointerId: 1 });
      fireEvent.pointerUp(stage, { clientX: 40, clientY: 10, pointerId: 1 });
    });

    expect(router.state.location.pathname).toBe("/library/youtube");
  });

  it("closes only via close button", async () => {
    renderMiniPlayer();

    await waitFor(() => {
      expect(screen.getByLabelText("Close")).toBeTruthy();
    });

    fireEvent.click(screen.getByLabelText("Close"));

    await waitFor(() => {
      expect(screen.queryByLabelText("Pause")).toBeNull();
    });
  });
});
