import { useEffect, useState } from "react";
import {
  ApiError,
  DEFAULT_MEDIA_PREFERENCES,
  type UserMediaPreferences,
} from "../api/client";
import { LANGUAGE_OPTIONS } from "../integrations/ytdlp";
import { useVaultApi } from "../context/VaultContext";

function LanguageChipList({
  languages,
  onChange,
}: {
  languages: string[];
  onChange: (next: string[]) => void;
}) {
  function toggle(code: string) {
    if (languages.includes(code)) {
      onChange(languages.filter((lang) => lang !== code));
    } else {
      onChange([...languages, code]);
    }
  }

  function move(index: number, delta: number) {
    const next = [...languages];
    const target = index + delta;
    if (target < 0 || target >= next.length) return;
    [next[index], next[target]] = [next[target], next[index]];
    onChange(next);
  }

  return (
    <div className="lang-pref-list">
      {languages.map((code, index) => (
        <div className="lang-pref-row" key={`${code}-${index}`}>
          <span className="lang-pref-order">{index + 1}</span>
          <span className="lang-pref-label">
            {LANGUAGE_OPTIONS.find((o) => o.value === code)?.label ?? code}
          </span>
          <div className="lang-pref-actions">
            <button
              type="button"
              className="secondary"
              disabled={index === 0}
              onClick={() => move(index, -1)}
              aria-label="Move up"
            >
              ↑
            </button>
            <button
              type="button"
              className="secondary"
              disabled={index === languages.length - 1}
              onClick={() => move(index, 1)}
              aria-label="Move down"
            >
              ↓
            </button>
            <button
              type="button"
              className="secondary"
              onClick={() => toggle(code)}
              aria-label="Remove"
            >
              ×
            </button>
          </div>
        </div>
      ))}
      <div className="lang-pref-add">
        {LANGUAGE_OPTIONS.filter((o) => !languages.includes(o.value)).map((opt) => (
          <button
            key={opt.value}
            type="button"
            className="secondary"
            onClick={() => onChange([...languages, opt.value])}
          >
            + {opt.label}
          </button>
        ))}
      </div>
    </div>
  );
}

export default function MediaPreferencesPanel() {
  const api = useVaultApi();
  const [prefs, setPrefs] = useState<UserMediaPreferences>(DEFAULT_MEDIA_PREFERENCES);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      setLoading(true);
      try {
        setPrefs(await api.getMediaPreferences());
        setError(null);
      } catch (err) {
        setError(
          err instanceof ApiError ? err.message : "Failed to load preferences",
        );
      } finally {
        setLoading(false);
      }
    })();
  }, [api]);

  async function save(event: React.FormEvent) {
    event.preventDefault();
    setSaving(true);
    setMessage(null);
    setError(null);
    try {
      setPrefs(await api.setMediaPreferences(prefs));
      setMessage("Preferences saved");
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Failed to save");
    } finally {
      setSaving(false);
    }
  }

  if (loading) {
    return <div className="loading-state">Loading media preferences...</div>;
  }

  return (
    <section className="panel media-prefs-panel">
      <div className="detail-header">
        <div>
          <h3>Media preferences</h3>
          <p className="card-meta">
            Default languages and quality for new archives. Applies across all
            integrations, not just yt-dlp.
          </p>
        </div>
      </div>

      <form className="media-prefs-form" onSubmit={(e) => void save(e)}>
        <fieldset>
          <legend>Subtitles</legend>
          <label className="field-label">
            Mode
            <select
              value={prefs.subtitle_mode}
              onChange={(e) =>
                setPrefs({ ...prefs, subtitle_mode: e.target.value })
              }
            >
              <option value="preferred">
                Preferred languages (from list below)
              </option>
              <option value="all">All available</option>
              <option value="manual">Manual only</option>
              <option value="none">None</option>
            </select>
          </label>
          <p className="field-hint">
            &quot;Preferred&quot; downloads subtitles matching your language
            priority (first match wins).
          </p>
          {prefs.subtitle_mode === "preferred" ? (
            <label className="field-label">
              Subtitle language priority
              <LanguageChipList
                languages={prefs.subtitle_languages}
                onChange={(subtitle_languages) =>
                  setPrefs({ ...prefs, subtitle_languages })
                }
              />
            </label>
          ) : null}
          <label className="checkbox-row">
            <input
              type="checkbox"
              checked={prefs.automatic_subtitles}
              onChange={(e) =>
                setPrefs({ ...prefs, automatic_subtitles: e.target.checked })
              }
            />
            Include auto-generated subtitles when matching preferred languages
          </label>
        </fieldset>

        <fieldset>
          <legend>Audio</legend>
          <label className="field-label">
            Mode
            <select
              value={prefs.audio_mode}
              onChange={(e) =>
                setPrefs({ ...prefs, audio_mode: e.target.value })
              }
            >
              <option value="preferred">
                Preferred languages (from list below)
              </option>
              <option value="main">Main track only</option>
              <option value="all">All tracks</option>
              <option value="none">None</option>
            </select>
          </label>
          {prefs.audio_mode === "preferred" ? (
            <label className="field-label">
              Audio language priority
              <LanguageChipList
                languages={prefs.audio_languages}
                onChange={(audio_languages) =>
                  setPrefs({ ...prefs, audio_languages })
                }
              />
            </label>
          ) : null}
        </fieldset>

        <fieldset>
          <legend>Video</legend>
          <label className="field-label">
            Default quality
            <select
              value={prefs.video_quality}
              onChange={(e) =>
                setPrefs({ ...prefs, video_quality: e.target.value })
              }
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
              checked={prefs.thumbnail}
              onChange={(e) =>
                setPrefs({ ...prefs, thumbnail: e.target.checked })
              }
            />
            Download source thumbnails by default
          </label>
        </fieldset>

        <button className="primary" type="submit" disabled={saving}>
          {saving ? "Saving..." : "Save preferences"}
        </button>
      </form>

      {message ? <p className="card-meta ok-text">{message}</p> : null}
      {error ? <p className="error-state">{error}</p> : null}
    </section>
  );
}
