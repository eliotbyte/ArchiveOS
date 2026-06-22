import type { EntityAsset } from "../../api/client";
import {
  formatPlaybackSpeed,
  getSettingsSections,
  PLAYBACK_SPEEDS,
  type SettingsPanel,
} from "./playerSettings";

interface PlayerSettingsMenuProps {
  assets: EntityAsset[];
  playbackRate: number;
  settingsPanel: SettingsPanel;
  onPanelChange: (panel: SettingsPanel) => void;
  onPlaybackRateChange: (rate: number) => void;
  onClose: () => void;
}

export default function PlayerSettingsMenu({
  assets,
  playbackRate,
  settingsPanel,
  onPanelChange,
  onPlaybackRateChange,
  onClose,
}: PlayerSettingsMenuProps) {
  const { showSubtitles, showAudioTracks } = getSettingsSections(assets);

  if (settingsPanel === "speed") {
    return (
      <div className="video-player-settings-menu" role="menu">
        <button
          type="button"
          className="video-player-settings-back"
          onClick={() => onPanelChange("main")}
        >
          ‹ Playback speed
        </button>
        {PLAYBACK_SPEEDS.map((speed) => (
          <button
            key={speed}
            type="button"
            className={`video-player-settings-item${playbackRate === speed ? " active" : ""}`}
            role="menuitemradio"
            aria-checked={playbackRate === speed}
            onClick={() => {
              onPlaybackRateChange(speed);
              onClose();
            }}
          >
            {formatPlaybackSpeed(speed)}
          </button>
        ))}
      </div>
    );
  }

  return (
    <div className="video-player-settings-menu" role="menu">
      <button
        type="button"
        className="video-player-settings-item video-player-settings-row"
        onClick={() => onPanelChange("speed")}
      >
        <span>Playback speed</span>
        <span className="video-player-settings-value">
          {formatPlaybackSpeed(playbackRate)}
        </span>
      </button>
      {showSubtitles ? (
        <button
          type="button"
          className="video-player-settings-item video-player-settings-row"
          onClick={() => onPanelChange("subtitles")}
        >
          <span>Subtitles</span>
          <span className="video-player-settings-value">Off</span>
        </button>
      ) : null}
      {showAudioTracks ? (
        <button
          type="button"
          className="video-player-settings-item video-player-settings-row"
          onClick={() => onPanelChange("audio")}
        >
          <span>Audio track</span>
          <span className="video-player-settings-value">Default</span>
        </button>
      ) : null}
    </div>
  );
}
