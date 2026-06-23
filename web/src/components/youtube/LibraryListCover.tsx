import type { EntityPreviewSummary } from "../../api/client";
import PreviewImage from "../PreviewImage";
import { libraryListMaterialIcon } from "./libraryListIcons";

interface LibraryListCoverProps {
  preview?: EntityPreviewSummary | null;
  title: string;
  listType: string;
  memberCount: number;
  overlay?: boolean;
  variant?: "card" | "header";
}

export default function LibraryListCover({
  preview,
  title,
  listType,
  memberCount,
  overlay = false,
  variant = "card",
}: LibraryListCoverProps) {
  const icon = libraryListMaterialIcon(listType);
  const smart = overlay || icon !== null;
  const coverClass =
    variant === "header" ? "youtube-playlist-cover" : "yt-playlist-cover";

  const cover = (
    <div className={`${coverClass}${smart ? " smart-overlay" : ""}`}>
      <PreviewImage preview={preview} title={title} compact />
      {smart && icon ? (
        <div className="yt-smart-cover-badge" aria-hidden="true">
          <span className="material-symbols-outlined">{icon}</span>
          <span className="yt-smart-cover-count">{memberCount}</span>
        </div>
      ) : null}
    </div>
  );

  if (variant === "header" || smart) {
    return cover;
  }

  return (
    <div className="yt-playlist-stack" aria-hidden="true">
      <div className="yt-playlist-layer yt-playlist-layer-back" />
      <div className="yt-playlist-layer yt-playlist-layer-mid" />
      {cover}
    </div>
  );
}
