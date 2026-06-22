import type { EntityAsset } from "../../api/client";

export const PLAYBACK_SPEEDS = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 1.75, 2] as const;

export type SettingsPanel = "main" | "speed" | "subtitles" | "audio";

export interface SettingsSections {
  showSubtitles: boolean;
  showAudioTracks: boolean;
}

export function getSettingsSections(assets: EntityAsset[]): SettingsSections {
  const subtitles = assets.filter(
    (asset) => asset.kind === "subtitle" && asset.status === "present",
  );
  const audioTracks = assets.filter(
    (asset) => asset.kind === "audio" && asset.status === "present",
  );
  return {
    showSubtitles: subtitles.length > 0,
    showAudioTracks: audioTracks.length > 1,
  };
}

export function formatPlaybackSpeed(rate: number): string {
  if (rate === 1) return "Normal";
  return `${rate}×`;
}
