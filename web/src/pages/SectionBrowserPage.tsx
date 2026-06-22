import { useEffect, useMemo, useState } from "react";
import { Link, Navigate, useParams } from "react-router-dom";
import { ApiError, type EntityListItem } from "../api/client";
import EntityCard from "../components/EntityCard";
import { useVaultApi } from "../context/VaultContext";
import {
  filterSectionItems,
  getSectionById,
  sectionBrowseParams,
} from "../library/sections";

export default function SectionBrowserPage() {
  const { sectionId } = useParams<{ sectionId: string }>();
  const section = sectionId ? getSectionById(sectionId) : undefined;
  const api = useVaultApi();
  const [items, setItems] = useState<EntityListItem[]>([]);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const visibleItems = useMemo(
    () => (section ? filterSectionItems(items, section) : []),
    [items, section],
  );

  useEffect(() => {
    if (!section || section.viewMode === "youtube") return;

    void (async () => {
      setLoading(true);
      setError(null);
      try {
        const result = await api.listEntities(sectionBrowseParams(section, query.trim()));
        setItems(result);
      } catch (err) {
        setError(err instanceof ApiError ? err.message : "Failed to load section");
        setItems([]);
      } finally {
        setLoading(false);
      }
    })();
  }, [api, query, section?.id, section?.viewMode]);

  if (!section) {
    return <Navigate to="/" replace />;
  }

  if (section.viewMode === "youtube") {
    return <Navigate to="/library/youtube" replace />;
  }

  const gridClass =
    section.viewMode === "gallery" ? "grid grid-gallery" : "grid";

  return (
    <section>
      <Link className="back-link" to="/">
        ← Library
      </Link>

      <div className="page-header">
        <h2>{section.label}</h2>
        <p className="subtitle">{section.description}</p>
        {section.placeholder ? (
          <p className="section-placeholder-note">
            This section is a preset. Movie and series filtering will arrive in a
            later pass.
          </p>
        ) : null}
      </div>

      <form
        className="toolbar"
        onSubmit={(event) => {
          event.preventDefault();
        }}
      >
        <input
          placeholder="Search titles and metadata"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        <button
          className="secondary"
          type="button"
          onClick={() => setQuery("")}
        >
          Reset
        </button>
      </form>

      {loading ? <div className="loading-state">Loading {section.label}...</div> : null}
      {error ? <div className="error-state">{error}</div> : null}
      {!loading && !error && visibleItems.length === 0 ? (
        <div className="empty-state">No items in this section yet.</div>
      ) : null}

      <div className={gridClass}>
        {visibleItems.map((entity) => (
          <EntityCard key={entity.id} entity={entity} />
        ))}
      </div>
    </section>
  );
}
