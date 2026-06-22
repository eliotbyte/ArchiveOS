import {
  displayTitle,
  type CollectionMemberItem,
  type EntityDetail,
  type EntityListItem,
} from "../api/client";

export interface QueueItem {
  entityId: string;
  title: string;
  primaryAssetId: string;
  preview?: { asset_id: string } | null;
  channel?: string | null;
  uploader?: string | null;
  duration?: string | null;
}

export interface PlayerQueueState {
  items: QueueItem[];
  currentIndex: number;
  shuffled: boolean;
  shuffleOrder: number[];
}

export function queueItemFromEntity(entity: EntityListItem): QueueItem | null {
  if (!entity.primary_asset_id || entity.primary_asset_status !== "present") {
    return null;
  }
  return {
    entityId: entity.id,
    title: displayTitle(entity.title, undefined, entity.id),
    primaryAssetId: entity.primary_asset_id,
    preview: entity.preview,
    channel: entity.channel ?? null,
    uploader: entity.uploader ?? null,
    duration: entity.duration ?? null,
  };
}

export function queueItemFromMember(member: CollectionMemberItem): QueueItem | null {
  if (!member.primary_asset_id || member.primary_asset_status !== "present") {
    return null;
  }
  return {
    entityId: member.id,
    title: displayTitle(member.title, undefined, member.id),
    primaryAssetId: member.primary_asset_id,
    preview: member.preview,
    channel: member.channel,
    uploader: member.uploader,
    duration: member.duration,
  };
}

export function queueItemFromDetail(entity: EntityDetail): QueueItem | null {
  const primary = entity.assets.find(
    (asset) => asset.role === "primary" && asset.status === "present",
  );
  if (!primary) return null;
  return {
    entityId: entity.id,
    title: displayTitle(entity.title, entity.metadata, entity.id),
    primaryAssetId: primary.id,
    preview: entity.preview,
    channel: entity.metadata.channel ?? null,
    uploader: entity.metadata.uploader ?? null,
    duration: entity.metadata.duration ?? null,
  };
}

export function buildQueueFromMembers(
  members: CollectionMemberItem[],
): QueueItem[] {
  return members
    .map(queueItemFromMember)
    .filter((item): item is QueueItem => item !== null);
}

export function shuffleIndices(length: number): number[] {
  const order = Array.from({ length }, (_, index) => index);
  for (let i = order.length - 1; i > 0; i -= 1) {
    const j = Math.floor(Math.random() * (i + 1));
    [order[i], order[j]] = [order[j], order[i]];
  }
  return order;
}

export function resolveQueueIndex(state: PlayerQueueState): number {
  if (state.shuffled && state.shuffleOrder.length === state.items.length) {
    return state.shuffleOrder[state.currentIndex] ?? 0;
  }
  return state.currentIndex;
}

export function currentQueueItem(state: PlayerQueueState): QueueItem | null {
  if (state.items.length === 0) return null;
  const index = resolveQueueIndex(state);
  return state.items[index] ?? null;
}

export function formatDurationLabel(
  raw?: string | null,
  seconds?: number,
): string {
  const total =
    seconds ??
    (raw ? Number.parseFloat(raw) : Number.NaN);
  if (!Number.isFinite(total) || total < 0) return "";
  const mins = Math.floor(total / 60);
  const secs = Math.floor(total % 60);
  return `${mins}:${secs.toString().padStart(2, "0")}`;
}
