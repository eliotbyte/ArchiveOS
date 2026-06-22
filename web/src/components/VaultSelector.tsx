import { useEffect, useState } from "react";
import { ApiError, type VaultRegistryEntry } from "../api/client";
import { useVault } from "../context/VaultContext";

export default function VaultSelector() {
  const { vault, setVault, api } = useVault();
  const [vaults, setVaults] = useState<VaultRegistryEntry[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        setVaults(await api.listVaults());
        setError(null);
      } catch (err) {
        setError(err instanceof ApiError ? err.message : "Failed to load vaults");
      }
    })();
  }, [api]);

  if (error) {
    return <span className="vault-selector error">{vault}</span>;
  }

  if (vaults.length <= 1) {
    return <span className="vault-selector">{vault}</span>;
  }

  return (
    <label className="vault-selector">
      <span className="vault-label">Vault</span>
      <select value={vault} onChange={(e) => setVault(e.target.value)}>
        {vaults.map((entry) => (
          <option key={entry.name} value={entry.name}>
            {entry.name}
          </option>
        ))}
      </select>
    </label>
  );
}
