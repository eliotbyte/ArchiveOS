import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import App from "./App";
import { VaultProvider } from "./context/VaultContext";
import "./index.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <BrowserRouter>
      <VaultProvider>
        <App />
      </VaultProvider>
    </BrowserRouter>
  </StrictMode>,
);
