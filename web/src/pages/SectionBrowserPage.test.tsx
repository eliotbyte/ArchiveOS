import { render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import SectionBrowserPage from "./SectionBrowserPage";

const listEntities = vi.fn();

const mockApi = { listEntities };

vi.mock("../context/VaultContext", () => ({
  useVaultApi: () => mockApi,
}));

describe("SectionBrowserPage", () => {
  beforeEach(() => {
    listEntities.mockReset();
    listEntities.mockResolvedValue([]);
  });

  it("loads entities using section-specific browse params", async () => {
    render(
      <MemoryRouter initialEntries={["/library/pictures"]}>
        <Routes>
          <Route path="/library/:sectionId" element={<SectionBrowserPage />} />
        </Routes>
      </MemoryRouter>,
    );

    await waitFor(() => {
      expect(listEntities).toHaveBeenCalledWith({
        limit: 100,
        kind: "image",
      });
    });

    expect(screen.getByRole("heading", { name: "Pictures" })).toBeTruthy();
  });

  it("redirects unknown sections to library home", async () => {
    render(
      <MemoryRouter initialEntries={["/library/unknown"]}>
        <Routes>
          <Route path="/" element={<div>Home</div>} />
          <Route path="/library/:sectionId" element={<SectionBrowserPage />} />
        </Routes>
      </MemoryRouter>,
    );

    await waitFor(() => {
      expect(screen.getByText("Home")).toBeTruthy();
    });
  });
});
