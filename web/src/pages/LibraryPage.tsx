import { useEffect, useState } from "react";
import { ApiError, type EntityListItem } from "../api/client";
import { useVaultApi } from "../context/VaultContext";
import EntityCard from "../components/EntityCard";

const KIND_OPTIONS = ["", "video", "image", "audio"] as const;

export default function LibraryPage() {
  const api = useVaultApi();
  const [items, setItems] = useState<EntityListItem[]>([]);
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState("");
  const [source, setSource] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  async function loadItems(search: string, kindFilter: string, sourceFilter: string) {
    setLoading(true);
    setError(null);
    try {
      const params: Record<string, string | number> = { limit: 100 };
      if (search) params.query = search;
      if (kindFilter) params.kind = kindFilter;
      if (sourceFilter) params.source = sourceFilter;
      setItems(await api.listEntities(params));
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Failed to load library");
      setItems([]);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void loadItems(query.trim(), kind, source);
  }, [api, query, kind, source]);

  function submit(event: React.FormEvent) {
    event.preventDefault();
    void loadItems(query.trim(), kind, source);
  }

  const sources = Array.from(
    new Set(items.map((item) => item.source).filter(Boolean) as string[]),
  ).sort();

  return (
    <section>
      <div className="page-header">
        <h2>Library</h2>
        <p className="subtitle">Browse archived content across your vault.</p>
      </div>

      <form className="toolbar" onSubmit={submit}>
        <input
          placeholder="Search titles/metadata"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        <select value={kind} onChange={(event) => setKind(event.target.value)}>
          {KIND_OPTIONS.map((option) => (
            <option key={option || "all"} value={option}>
              {option ? option : "All types"}
            </option>
          ))}
        </select>
        <select value={source} onChange={(event) => setSource(event.target.value)}>
          <option value="">All sources</option>
          {sources.map((value) => (
            <option key={value} value={value}>
              {value}
            </option>
          ))}
        </select>
        <button className="secondary" type="submit">
          Search
        </button>
        <button
          className="secondary"
          type="button"
          onClick={() => {
            setQuery("");
            setKind("");
            setSource("");
          }}
        >
          Reset
        </button>
      </form>

      {loading ? <div className="loading-state">Loading library...</div> : null}
      {error ? <div className="error-state">{error}</div> : null}
      {!loading && !error && items.length === 0 ? (
        <div className="empty-state">
          No content in this vault yet. Import files to inbox or use Integrations to
          enrich the library.
        </div>
      ) : null}

      <div className="grid">
        {items.map((entity) => (
          <EntityCard key={entity.id} entity={entity} />
        ))}
      </div>
    </section>
  );
}
