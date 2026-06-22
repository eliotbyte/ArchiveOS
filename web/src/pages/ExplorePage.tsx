import { useEffect, useState } from "react";
import { api, ApiError, type EntityListItem } from "../api/client";
import EntityCard from "../components/EntityCard";

export default function ExplorePage() {
  const [items, setItems] = useState<EntityListItem[]>([]);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void loadItems("");
  }, []);

  async function loadItems(search: string) {
    setLoading(true);
    setError(null);
    try {
      const result = search
        ? await api.listEntities({ query: search, limit: 100 })
        : await api.listEntities({ limit: 100 });
      setItems(result);
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Failed to load entities");
      setItems([]);
    } finally {
      setLoading(false);
    }
  }

  function submit(event: React.FormEvent) {
    event.preventDefault();
    void loadItems(query.trim());
  }

  return (
    <section>
      <form className="toolbar" onSubmit={submit}>
        <input
          placeholder="Search titles/metadata"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        <button className="secondary" type="submit">
          Search
        </button>
        <button
          className="secondary"
          type="button"
          onClick={() => {
            setQuery("");
            void loadItems("");
          }}
        >
          Reset
        </button>
      </form>

      {loading ? <div className="loading-state">Loading archive...</div> : null}
      {error ? <div className="error-state">{error}</div> : null}
      {!loading && !error && items.length === 0 ? (
        <div className="empty-state">No entities yet. Archive a URL to get started.</div>
      ) : null}

      <div className="grid">
        {items.map((entity) => (
          <EntityCard key={entity.id} entity={entity} />
        ))}
      </div>
    </section>
  );
}
