import { describe, expect, it } from "vitest";
import {
  buildQueueFromMembers,
  currentQueueItem,
  formatDurationLabel,
  queueItemFromEntity,
  shuffleIndices,
  type PlayerQueueState,
} from "./queue";

describe("player queue", () => {
  it("builds queue from playable collection members only", () => {
    const queue = buildQueueFromMembers([
      {
        id: "1",
        title: "A",
        status: "present",
        position: 0,
        primary_asset_id: "asset-1",
        primary_asset_status: "present",
      },
      {
        id: "2",
        title: "B",
        status: "present",
        position: 1,
        primary_asset_id: null,
        primary_asset_status: null,
      },
    ]);

    expect(queue).toHaveLength(1);
    expect(queue[0]?.entityId).toBe("1");
  });

  it("returns current queue item using shuffle order", () => {
    const state: PlayerQueueState = {
      items: [
        { entityId: "a", title: "A", primaryAssetId: "1" },
        { entityId: "b", title: "B", primaryAssetId: "2" },
      ],
      currentIndex: 0,
      shuffled: true,
      shuffleOrder: [1, 0],
    };

    expect(currentQueueItem(state)?.entityId).toBe("b");
  });

  it("formats duration labels", () => {
    expect(formatDurationLabel("125")).toBe("2:05");
    expect(formatDurationLabel(null, 61)).toBe("1:01");
  });

  it("builds queue item from entity list item with channel metadata", () => {
    const item = queueItemFromEntity({
      id: "1",
      title: "A",
      status: "present",
      tags: [],
      primary_asset_id: "asset-1",
      primary_asset_status: "present",
      channel: "Test Channel",
      uploader: "Uploader",
      duration: "90",
    });
    expect(item?.channel).toBe("Test Channel");
    expect(item?.uploader).toBe("Uploader");
    expect(item?.duration).toBe("90");
  });

  it("creates a permutation shuffle order", () => {
    const order = shuffleIndices(4);
    expect(order).toHaveLength(4);
    expect(new Set(order)).toEqual(new Set([0, 1, 2, 3]));
  });
});
