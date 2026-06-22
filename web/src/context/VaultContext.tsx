import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { createApi, DEFAULT_VAULT_NAME } from "../api/client";

const STORAGE_KEY = "archiveos.vault";

interface VaultContextValue {
  vault: string;
  setVault: (name: string) => void;
  api: ReturnType<typeof createApi>;
  assetContentUrl: (assetId: string) => string;
}

const VaultContext = createContext<VaultContextValue | null>(null);

export function VaultProvider({ children }: { children: ReactNode }) {
  const [vault, setVaultState] = useState(() => {
    if (typeof window === "undefined") return DEFAULT_VAULT_NAME;
    return localStorage.getItem(STORAGE_KEY) ?? DEFAULT_VAULT_NAME;
  });

  const setVault = useCallback((name: string) => {
    setVaultState(name);
    localStorage.setItem(STORAGE_KEY, name);
  }, []);

  const api = useMemo(() => createApi(vault), [vault]);
  const assetContentUrl = useMemo(() => api.assetContentUrl, [api]);

  return (
    <VaultContext.Provider value={{ vault, setVault, api, assetContentUrl }}>
      {children}
    </VaultContext.Provider>
  );
}

export function useVault() {
  const ctx = useContext(VaultContext);
  if (!ctx) {
    throw new Error("useVault must be used within VaultProvider");
  }
  return ctx;
}

export function useVaultApi() {
  return useVault().api;
}
