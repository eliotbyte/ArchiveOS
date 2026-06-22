import { useState } from "react";
import { ApiError } from "../api/client";
import { useVaultApi } from "../context/VaultContext";

interface ArchiveFormProps {
  onSubmitted?: () => void;
}

export default function ArchiveForm({ onSubmitted }: ArchiveFormProps) {
  const api = useVaultApi();
  const [url, setUrl] = useState("");
  const [mode, setMode] = useState<"once" | "subscription">("once");
  const [intervalMinutes, setIntervalMinutes] = useState(60);
  const [videoQuality, setVideoQuality] = useState("best");
  const [thumbnail, setThumbnail] = useState(true);
  const [subtitles, setSubtitles] = useState("preferred");
  const [automaticSubtitles, setAutomaticSubtitles] = useState(true);
  const [audioTracks, setAudioTracks] = useState("main");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    if (!url.trim()) return;

    setBusy(true);
    setMessage(null);
    setError(null);
    const asset_policy = {
      video: videoQuality,
      thumbnail,
      subtitles,
      subtitle_languages: ["original", "en", "ru"],
      automatic_subtitles: automaticSubtitles,
      audio_tracks: audioTracks,
    };

    try {
      if (mode === "subscription") {
        const sub = await api.subscribeUrl({
          url: url.trim(),
          interval_minutes: intervalMinutes,
          asset_policy,
        });
        setMessage(`Monitor created: ${sub.id}`);
      } else {
        const job = await api.archiveUrl({ url: url.trim(), asset_policy });
        setMessage(`Archive job queued: ${job.id}`);
      }
      setUrl("");
      onSubmitted?.();
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Archive failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="panel">
      <form className="archive-form-grid" onSubmit={submit}>
        <input
          type="url"
          placeholder="Paste video or playlist URL"
          value={url}
          onChange={(event) => setUrl(event.target.value)}
        />
        <select value={mode} onChange={(event) => setMode(event.target.value as "once" | "subscription")}>
          <option value="once">Archive once</option>
          <option value="subscription">Monitor playlist/channel</option>
        </select>
        {mode === "subscription" ? (
          <input
            type="number"
            min={5}
            value={intervalMinutes}
            onChange={(event) => setIntervalMinutes(Number(event.target.value))}
            placeholder="Interval minutes"
          />
        ) : null}
        <select value={videoQuality} onChange={(event) => setVideoQuality(event.target.value)}>
          <option value="best">Video: best</option>
          <option value="1080">Video: 1080p</option>
          <option value="720">Video: 720p</option>
        </select>
        <label className="checkbox-row">
          <input
            type="checkbox"
            checked={thumbnail}
            onChange={(event) => setThumbnail(event.target.checked)}
          />
          Import source thumbnail
        </label>
        <select value={subtitles} onChange={(event) => setSubtitles(event.target.value)}>
          <option value="preferred">Subtitles: preferred</option>
          <option value="all">Subtitles: all</option>
          <option value="none">Subtitles: none</option>
        </select>
        <label className="checkbox-row">
          <input
            type="checkbox"
            checked={automaticSubtitles}
            onChange={(event) => setAutomaticSubtitles(event.target.checked)}
          />
          Automatic subtitles
        </label>
        <select value={audioTracks} onChange={(event) => setAudioTracks(event.target.value)}>
          <option value="main">Audio: main</option>
          <option value="all">Audio: all</option>
          <option value="none">Audio: none</option>
        </select>
        <button className="primary" type="submit" disabled={busy || !url.trim()}>
          {busy ? "Submitting..." : mode === "subscription" ? "Create monitor" : "Archive"}
        </button>
      </form>
      {message ? <p className="card-meta">{message}</p> : null}
      {error ? <p className="error-state">{error}</p> : null}
    </section>
  );
}
