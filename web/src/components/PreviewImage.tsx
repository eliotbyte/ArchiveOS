import { useState } from "react";
import {
  previewVisualState,
  type EntityPreviewSummary,
} from "../api/client";
import { useVault } from "../context/VaultContext";

interface PreviewImageProps {
  preview?: EntityPreviewSummary | null;
  title?: string | null;
  compact?: boolean;
}

export default function PreviewImage({
  preview,
  title,
  compact = false,
}: PreviewImageProps) {
  const { assetContentUrl } = useVault();
  const [imageFailed, setImageFailed] = useState(false);
  const state = previewVisualState(preview, imageFailed);

  if (state === "missing") {
    return (
      <div className="preview-frame">
        <div className={`preview-state ${compact ? "" : "missing"}`}>
          No preview
        </div>
      </div>
    );
  }

  if (state === "pending") {
    return (
      <div className="preview-frame">
        <div className="preview-state pending">
          Preview {preview?.status ?? "pending"}
        </div>
      </div>
    );
  }

  if (state === "failed") {
    return (
      <div className="preview-frame">
        <div className="preview-state failed">Preview failed to load</div>
      </div>
    );
  }

  return (
    <div className="preview-frame">
      {!imageFailed ? (
        <img
          src={assetContentUrl(preview!.asset_id)}
          alt={title ?? "Preview"}
          loading="lazy"
          onError={() => setImageFailed(true)}
        />
      ) : (
        <div className="preview-state failed">Preview failed to load</div>
      )}
    </div>
  );
}
