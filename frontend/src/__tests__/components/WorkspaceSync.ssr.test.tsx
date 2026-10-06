/**
 * #1369 — SSR / hydration safety for the workspace shell.
 *
 * `readWorkspace` hits `localStorage` synchronously. Seeding `useState` from
 * it makes the first client render depend on browser storage the server never
 * saw (a favorites count the server could not produce). These tests pin the
 * contract: the first render is the canonical empty snapshot, and the
 * persisted bucket is adopted only after mount.
 */

import React from "react";
import { render, screen, act } from "@testing-library/react";
import OfflineProvider from "@/components/providers/OfflineProvider";
import { WorkspaceProvider, useWorkspace } from "@/components/providers/WorkspaceProvider";
import { WalletProvider } from "@/components/providers/WalletProvider";
import {
  workspaceBucket,
  writeWorkspace,
} from "@/lib/sync/workspaceStore";
import { emptySnapshot } from "@/lib/sync/merge";

function FavoritesProbe() {
  const { snapshot } = useWorkspace();
  return <span data-testid="favorites">{snapshot.favorites.length}</span>;
}

function Shell({ children }: { children: React.ReactNode }) {
  return (
    <WalletProvider>
      <OfflineProvider>
        <WorkspaceProvider enabled={false}>{children}</WorkspaceProvider>
      </OfflineProvider>
    </WalletProvider>
  );
}

describe("useWorkspaceSync SSR hydration safety (#1369)", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  afterEach(() => {
    window.localStorage.clear();
  });

  it("renders the empty snapshot on the server even with persisted favorites", () => {
    const bucket = workspaceBucket(null);
    const seeded = emptySnapshot();
    seeded.favorites = ["template-a", "template-b"];
    writeWorkspace(bucket, seeded);

    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const { renderToString } = require("react-dom/server");
    const html = renderToString(
      <Shell>
        <FavoritesProbe />
      </Shell>,
    );

    expect(html).toContain('data-testid="favorites">0<');
  });

  it("adopts the persisted bucket after mount", async () => {
    const bucket = workspaceBucket(null);
    const seeded = emptySnapshot();
    seeded.favorites = ["template-a"];
    writeWorkspace(bucket, seeded);

    render(
      <Shell>
        <FavoritesProbe />
      </Shell>,
    );

    await act(async () => {
      await Promise.resolve();
    });

    expect(screen.getByTestId("favorites")).toHaveTextContent("1");
  });
});
