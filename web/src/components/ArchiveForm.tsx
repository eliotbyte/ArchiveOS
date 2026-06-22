import { useState } from "react";
import { api, ApiError } from "../api/client";

export default function ArchiveForm() {
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    if (!url.trim()) return;

    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      const job = await api.archiveUrl(url.trim());
      setMessage(`Archive job queued: ${job.id}`);
      setUrl("");
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Archive failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="panel">
      <form className="archive-form" onSubmit={submit}>
        <input
          type="url"
          placeholder="Paste video URL to archive"
          value={url}
          onChange={(event) => setUrl(event.target.value)}
        />
        <button className="primary" type="submit" disabled={busy || !url.trim()}>
          {busy ? "Archiving..." : "Archive"}
        </button>
      </form>
      {message ? <p className="card-meta">{message}</p> : null}
      {error ? <p className="error-state">{error}</p> : null}
    </section>
  );
}
