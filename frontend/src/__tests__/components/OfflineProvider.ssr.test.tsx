/**
 * #1369 — SSR / hydration safety for the offline shell.
 *
 * `createOfflineEngine` hydrates its outbox from localStorage while it is being
 * constructed. If the provider seeded its first render from that engine, the
 * client would render a queued count the server could never have produced, and
 * React would report a hydration mismatch. These tests pin the contract: the
 * first render must be storage-free, and the persisted state must be adopted
 * immediately after mount.
 */

import React from "react";
import { render, screen, act } from "@testing-library/react";
import OfflineProvider, { useOffline } from "@/components/providers/OfflineProvider";
import { OUTBOX_STORAGE_KEY } from "@/lib/offline/outbox";

function QueueProbe() {
  const { queuedCount, status } = useOffline();
  return (
    <div>
      <span data-testid="queued">{queuedCount}</span>
      <span data-testid="status">{status}</span>
    </div>
  );
}

function persistQueuedOperation() {
  window.localStorage.setItem(
    OUTBOX_STORAGE_KEY,
    JSON.stringify({
      operations: [
        {
          id: "op-1",
          kind: "workspace.push",
          payload: { walletAddress: "GABC" },
          queuedAt: 1_700_000_000_000,
          attempts: 0,
        },
      ],
      lastDrainedAt: null,
      lastError: null,
    }),
  );
}

describe("OfflineProvider SSR hydration safety (#1369)", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  // This suite deliberately seeds the outbox. Without an explicit teardown the
  // jsdom localStorage is shared with whatever suite runs next in the same
  // process, and a leftover pending operation leaks into unrelated tests.
  afterEach(() => {
    window.localStorage.clear();
  });

  it("does not read the outbox while rendering", () => {
    persistQueuedOperation();
    const getItem = jest.spyOn(Storage.prototype, "getItem");

    // `renderToString` is the server pass: it must never touch browser storage.
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const { renderToString } = require("react-dom/server");
    const html = renderToString(
      <OfflineProvider>
        <QueueProbe />
      </OfflineProvider>,
    );

    expect(html).toContain("0");
    const outboxReads = getItem.mock.calls.filter(
      ([key]) => key === OUTBOX_STORAGE_KEY,
    );
    expect(outboxReads).toHaveLength(0);

    getItem.mockRestore();
  });

  it("renders the SSR default even when work is queued", () => {
    persistQueuedOperation();

    // This is the markup the server produces. A client first render that showed
    // "1 queued" here is exactly the hydration mismatch #1369 describes.
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const { renderToString } = require("react-dom/server");
    const html = renderToString(
      <OfflineProvider>
        <QueueProbe />
      </OfflineProvider>,
    );

    expect(html).toContain('data-testid="queued">0<');
    expect(html).toContain('data-testid="status">unknown<');
  });

  it("adopts the persisted outbox after mount", async () => {
    persistQueuedOperation();

    render(
      <OfflineProvider>
        <QueueProbe />
      </OfflineProvider>,
    );

    await act(async () => {
      await Promise.resolve();
    });

    expect(screen.getByTestId("queued")).toHaveTextContent("1");
  });
});
