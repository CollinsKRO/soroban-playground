/**
 * #1369 — SSR / hydration safety for the theme shell.
 *
 * The persisted preference lives in `localStorage` and the OS theme in
 * `matchMedia` — neither exists on the server. Seeding React state from the
 * controller during render would make the client's first render depend on
 * browser storage the server never saw (e.g. a stored "light" preference
 * rendering light while the server rendered dark). These tests pin the
 * contract: the first render is the canonical SSR default, and the stored
 * preference is adopted only after mount.
 */

import React from "react";
import { render, screen, act } from "@testing-library/react";
import {
  ThemeProvider,
  useTheme,
  SSR_THEME_STATE,
} from "@/components/providers/ThemeProvider";
import { THEME_STORAGE_KEY } from "@/lib/theme/engine";
import { DEFAULT_THEME_MODE } from "@/lib/theme/tokens";

function ModeProbe() {
  const { state } = useTheme();
  return (
    <div>
      <span data-testid="mode">{state.mode}</span>
      <span data-testid="preference">{state.preference}</span>
      <span data-testid="source">{state.source}</span>
    </div>
  );
}

describe("ThemeProvider SSR hydration safety (#1369)", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  afterEach(() => {
    window.localStorage.clear();
  });

  it("exposes a static SSR default that matches the server", () => {
    expect(SSR_THEME_STATE.mode).toBe(DEFAULT_THEME_MODE);
    expect(SSR_THEME_STATE.preference).toBe("system");
  });

  it("renders the SSR default even when a preference is stored", () => {
    window.localStorage.setItem(THEME_STORAGE_KEY, "light");

    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const { renderToString } = require("react-dom/server");
    const html = renderToString(
      <ThemeProvider>
        <ModeProbe />
      </ThemeProvider>,
    );

    // Server HTML must carry the canonical default, never the stored value.
    expect(html).toContain(`data-testid="mode">${DEFAULT_THEME_MODE}<`);
  });

  it("does not read stored preference while rendering", () => {
    window.localStorage.setItem(THEME_STORAGE_KEY, "light");
    const getItem = jest.spyOn(Storage.prototype, "getItem");

    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const { renderToString } = require("react-dom/server");
    renderToString(
      <ThemeProvider>
        <ModeProbe />
      </ThemeProvider>,
    );

    const themeReads = getItem.mock.calls.filter(
      ([key]) => key === THEME_STORAGE_KEY,
    );
    // The server pass must never touch browser storage; the mount effect
    // adopts the stored value on the client instead.
    expect(themeReads).toHaveLength(0);

    getItem.mockRestore();
  });

  it("adopts the stored preference after mount", async () => {
    window.localStorage.setItem(THEME_STORAGE_KEY, "light");

    render(
      <ThemeProvider>
        <ModeProbe />
      </ThemeProvider>,
    );

    await act(async () => {
      await Promise.resolve();
    });

    expect(screen.getByTestId("mode")).toHaveTextContent("light");
    expect(screen.getByTestId("preference")).toHaveTextContent("light");
  });
});
