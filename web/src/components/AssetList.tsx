import { useState } from "react";
import { browserDisplayState, type EntityAsset } from "../api/client";
import { useVault } from "../context/VaultContext";

interface AssetListProps {
  entityId: string;
  assets: EntityAsset[];
  onChanged?: () => void;
}

export default function AssetList({ entityId, assets, onChanged }: AssetListProps) {
  const { api, assetContentUrl } = useVault();
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function acquire(asset: EntityAsset) {
    setBusyId(asset.id);
    setError(null);
    try {
      await api.acquireAsset(entityId, asset.id);
      onChanged?.();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Acquire failed");
    } finally {
      setBusyId(null);
    }
  }

  return (
    <section className="panel">
      <h3>Assets</h3>
      {error ? <p className="error-state">{error}</p> : null}
      <table className="asset-table">
        <thead>
          <tr>
            <th>Kind</th>
            <th>Status</th>
            <th>Storage</th>
            <th>MIME</th>
            <th>Browser</th>
            <th>Action</th>
          </tr>
        </thead>
        <tbody>
          {assets.map((asset) => {
            const display = browserDisplayState(asset.mime);
            return (
              <tr key={asset.id}>
                <td>
                  {asset.role}/{asset.kind}
                  {asset.metadata.preview_role ? ` (${asset.metadata.preview_role})` : ""}
                </td>
                <td>{asset.status}</td>
                <td>{asset.storage_strategy}</td>
                <td>{asset.mime ?? "—"}</td>
                <td>{display}</td>
                <td>
                  {asset.status === "remote" || asset.status === "missing_local" ? (
                    <button
                      className="secondary"
                      type="button"
                      disabled={busyId === asset.id}
                      onClick={() => acquire(asset)}
                    >
                      {busyId === asset.id ? "Acquiring..." : "Acquire"}
                    </button>
                  ) : asset.status === "present" && display === "supported" ? (
                    <a href={assetContentUrl(asset.id)} target="_blank" rel="noreferrer">
                      Open
                    </a>
                  ) : asset.status === "present" ? (
                    <span className="card-meta">Not displayable in browser</span>
                  ) : (
                    "—"
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </section>
  );
}
