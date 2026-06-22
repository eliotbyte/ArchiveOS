import { Link } from "react-router-dom";
import { COLLECTIONS_ENTRY, LIBRARY_SECTIONS } from "../library/sections";

export default function LibraryHomePage() {
  const entries = [...LIBRARY_SECTIONS, COLLECTIONS_ENTRY];

  return (
    <section>
      <div className="page-header">
        <h2>Library</h2>
        <p className="subtitle">
          Choose what you want to browse. Each section opens a focused view for
          that kind of content.
        </p>
      </div>

      <div className="section-grid">
        {entries.map((entry) => (
          <Link key={entry.id} className="section-card" to={entry.route}>
            <h3>{entry.label}</h3>
            <p>{entry.description}</p>
          </Link>
        ))}
      </div>
    </section>
  );
}
