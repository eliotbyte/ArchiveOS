import { useEffect, useState } from "react";
import {
  ApiError,
  DEFAULT_MEDIA_PREFERENCES,
  preferencesToAssetPolicy,
  type UserMediaPreferences,
} from "../api/client";
import {
  formatIntervalMinutes,
  languageLabel,
  MONITOR_INTERVAL_PRESETS,
} from "../integrations/ytdlp";
import { useVaultApi } from "../context/VaultContext";

interface ArchiveFormProps {
  onSubmitted?: () => void;
}

export default function ArchiveForm({ onSubmitted }: ArchiveFormProps) {
  const api = useVaultApi();
  const [prefs, setPrefs] = useState<UserMediaPreferences>(DEFAULT_MEDIA_PREFERENCES);
  const [url, setUrl] = useState("");
  const [mode, setMode] = useState<"once" | "subscription">("once");
  const [intervalPreset, setIntervalPreset] = useState("60");
  const [intervalMinutes, setIntervalMinutes] = useState(60);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [videoQuality, setVideoQuality] = useState("best");
  const [thumbnail, setThumbnail] = useState(true);
  const [subtitles, setSubtitles] = useState("preferred");
  const [automaticSubtitles, setAutomaticSubtitles] = useState(true);
  const [audioTracks, setAudioTracks] = useState("preferred");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        const loaded = await api.getMediaPreferences();
        setPrefs(loaded);
        setVideoQuality(loaded.video_quality);
        setThumbnail(loaded.thumbnail);
        setSubtitles(loaded.subtitle_mode);
        setAutomaticSubtitles(loaded.automatic_subtitles);
        setAudioTracks(loaded.audio_mode);
      } catch {
        setPrefs(DEFAULT_MEDIA_PREFERENCES);
      }
    })();
  }, [api]);

  function buildAssetPolicy() {
    if (!advancedOpen) {
      return preferencesToAssetPolicy(prefs);
    }
    return {
      video: videoQuality,
      thumbnail,
      subtitles,
      subtitle_languages: prefs.subtitle_languages,
      automatic_subtitles: automaticSubtitles,
      audio_tracks: audioTracks,
      audio_languages: prefs.audio_languages,
    };
  }

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    if (!url.trim()) return;

    setBusy(true);
    setMessage(null);
    setError(null);
    const asset_policy = buildAssetPolicy();

    try {
      if (mode === "subscription") {
        const sub = await api.subscribeUrl({
          url: url.trim(),
          interval_minutes: intervalMinutes,
          asset_policy,
        });
        setMessage(`Monitor created for ${sub.collection_title ?? "playlist"}`);
      } else {
        const job = await api.archiveUrl({ url: url.trim(), asset_policy });
        setMessage(`Archive job queued (${job.id.slice(0, 8)}…)`);
      }
      setUrl("");
      onSubmitted?.();
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Archive failed");
    } finally {
      setBusy(false);
    }
  }

  const subtitleHint =
    subtitles === "preferred"
      ? `Uses your language priority: ${prefs.subtitle_languages.map(languageLabel).join(" → ")}`
      : null;

  return (
    <section className="panel archive-form-panel">
      <form className="archive-form" onSubmit={submit}>
        <label className="field-label">
          URL
          <input
            type="url"
            placeholder="Paste video or playlist URL"
            value={url}
            onChange={(event) => setUrl(event.target.value)}
          />
        </label>

        <label className="field-label">
          Action
          <select
            value={mode}
            onChange={(event) =>
              setMode(event.target.value as "once" | "subscription")
            }
          >
            <option value="once">Archive once</option>
            <option value="subscription">Monitor playlist/channel</option>
          </select>
        </label>

        {mode === "subscription" ? (
          <>
            <label className="field-label">
              Check interval
              <select
                value={intervalPreset}
                onChange={(event) => {
                  const value = event.target.value;
                  setIntervalPreset(value);
                  if (value !== "custom") {
                    setIntervalMinutes(Number(value));
                  }
                }}
              >
                {MONITOR_INTERVAL_PRESETS.map((preset) => (
                  <option
                    key={preset.label}
                    value={preset.minutes > 0 ? String(preset.minutes) : "custom"}
                  >
                    {preset.label}
                  </option>
                ))}
              </select>
            </label>
            {intervalPreset === "custom" ? (
              <label className="field-label">
                Custom interval (minutes)
                <input
                  type="number"
                  min={5}
                  value={intervalMinutes}
                  onChange={(event) =>
                    setIntervalMinutes(Number(event.target.value))
                  }
                />
              </label>
            ) : (
              <p className="field-hint">{formatIntervalMinutes(intervalMinutes)}</p>
            )}
          </>
        ) : null}

        <button
          className="text-btn"
          type="button"
          onClick={() => setAdvancedOpen((open) => !open)}
        >
          {advancedOpen ? "Hide" : "Show"} advanced options
        </button>

        {advancedOpen ? (
          <div className="archive-form-advanced">
            <label className="field-label">
              Video quality
              <select
                value={videoQuality}
                onChange={(event) => setVideoQuality(event.target.value)}
              >
                <option value="best">Best available</option>
                <option value="best_1080p">Best up to 1080p</option>
                <option value="1080">1080p</option>
                <option value="720">720p</option>
              </select>
            </label>
            <label className="checkbox-row">
              <input
                type="checkbox"
                checked={thumbnail}
                onChange={(event) => setThumbnail(event.target.checked)}
              />
              Import source thumbnail
            </label>
            <label className="field-label">
              Subtitles
              <select
                value={subtitles}
                onChange={(event) => setSubtitles(event.target.value)}
              >
                <option value="preferred">Preferred languages</option>
                <option value="all">All</option>
                <option value="manual">Manual only</option>
                <option value="none">None</option>
              </select>
            </label>
            {subtitleHint ? <p className="field-hint">{subtitleHint}</p> : null}
            <label className="checkbox-row">
              <input
                type="checkbox"
                checked={automaticSubtitles}
                onChange={(event) => setAutomaticSubtitles(event.target.checked)}
              />
              Include auto-generated subtitles
            </label>
            <label className="field-label">
              Audio tracks
              <select
                value={audioTracks}
                onChange={(event) => setAudioTracks(event.target.value)}
              >
                <option value="preferred">Preferred languages</option>
                <option value="main">Main track</option>
                <option value="all">All</option>
                <option value="none">None</option>
              </select>
            </label>
            <p className="field-hint">
              Change default languages in Media preferences below.
            </p>
          </div>
        ) : (
          <p className="field-hint">
            Using your media preferences: {prefs.video_quality} video, subtitles{" "}
            {prefs.subtitle_mode} ({prefs.subtitle_languages.map(languageLabel).join(" → ")}).
          </p>
        )}

        <button className="primary" type="submit" disabled={busy || !url.trim()}>
          {busy
            ? "Submitting..."
            : mode === "subscription"
              ? "Create monitor"
              : "Archive"}
        </button>
      </form>
      {message ? <p className="card-meta ok-text">{message}</p> : null}
      {error ? <p className="error-state">{error}</p> : null}
    </section>
  );
}
