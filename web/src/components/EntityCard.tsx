import { Link } from "react-router-dom";
import type { EntityListItem } from "../api/client";
import PreviewImage from "./PreviewImage";

interface EntityCardProps {
  entity: EntityListItem;
}

export default function EntityCard({ entity }: EntityCardProps) {
  const title = entity.title ?? entity.id.slice(0, 8);

  return (
    <Link className="card" to={`/entities/${entity.id}`}>
      <PreviewImage preview={entity.preview} title={title} compact />
      <div className="card-body">
        <div className="card-title">{title}</div>
        <div className="card-meta">
          {[entity.kind, entity.source, entity.status]
            .filter(Boolean)
            .join(" · ")}
        </div>
        <div className="badge-row">
          {entity.primary_asset_status ? (
            <span className={`badge ${statusClass(entity.primary_asset_status)}`}>
              primary: {entity.primary_asset_status}
            </span>
          ) : null}
          {entity.mime ? <span className="badge">{entity.mime}</span> : null}
        </div>
      </div>
    </Link>
  );
}

function statusClass(status: string): string {
  if (status === "present") return "ok";
  if (status === "remote" || status === "missing_local") return "warn";
  return "bad";
}
