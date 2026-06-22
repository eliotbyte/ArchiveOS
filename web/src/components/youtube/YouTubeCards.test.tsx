import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { EntityListItem } from "../../api/client";
import YouTubeVideoCard from "./YouTubeCards";

vi.mock("../../context/VaultContext", () => ({
  useVault: () => ({
    assetContentUrl: (assetId: string) => `/assets/${assetId}`,
  }),
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
});
