import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";
import { COLLECTIONS_ENTRY, LIBRARY_SECTIONS } from "../library/sections";
import LibraryHomePage from "./LibraryHomePage";

describe("LibraryHomePage", () => {
  it("renders hardcoded section cards with routes", () => {
    render(
      <MemoryRouter>
        <LibraryHomePage />
      </MemoryRouter>,
    );

    for (const entry of [...LIBRARY_SECTIONS, COLLECTIONS_ENTRY]) {
      const link = screen
        .getAllByRole("link")
        .find((element) => element.getAttribute("href") === entry.route);
      expect(link).toBeTruthy();
      expect(link?.querySelector("h3")?.textContent).toBe(entry.label);
    }
  });
});
