import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, render } from "@testing-library/react";
import type { ReactElement } from "react";
import { connectEngineStream } from "../../connection.ts";
import { useEntitySource } from "../../hooks/useEntitySource.ts";
import { useRunStatus } from "../../hooks/useRunStatus.ts";
import { useStreamStatus } from "../../hooks/useStreamStatus.ts";
import { entity, frame, hello, rig } from "../../testing.ts";
import { EngineStreamProvider } from "./EngineStreamProvider.tsx";

afterEach(cleanup);

function setup() {
  const r = rig();
  const connection = connectEngineStream(r.options);
  r.socket().receive(hello());
  const renders = { status: 0, run: 0, source: 0 };
  function Status(): ReactElement {
    renders.status += 1;
    return <p data-testid="status">{useStreamStatus()}</p>;
  }
  function Run(): ReactElement {
    renders.run += 1;
    const s = useRunStatus();
    return <p data-testid="run">{s ? `${s.state}@${s.tick}` : "none"}</p>;
  }
  function Source(): ReactElement {
    renders.source += 1;
    return <p>{useEntitySource() === connection.source ? "source" : "other"}</p>;
  }
  const ui = render(
    <EngineStreamProvider connection={connection}>
      <Status />
      <Run />
      <Source />
    </EngineStreamProvider>,
  );
  const flush = (...messages: unknown[]) =>
    act(() => {
      for (const m of messages) r.socket().receive(m);
      r.clock.paint();
    });
  return { r, connection, renders, flush, ui };
}

describe("EngineStreamProvider", () => {
  test("10,000-entity frames: constant hook renders over 100 flushes", () => {
    const { flush, renders, connection } = setup();
    const entities = Array.from({ length: 10_000 }, (_, i) => entity(i + 1));
    flush(frame(0, 1, entities));
    const before = { ...renders };
    let notifications = 0;
    connection.store.subscribe(() => {
      notifications += 1;
    });
    for (let i = 1; i <= 100; i++) flush(frame(i, 1, entities, { cause: "control" })); // run unchanged
    expect(notifications).toBe(100); // one store update per flush
    expect(renders).toEqual(before); // status, run and source consumers: no React render for any of them
    expect(renders.source).toBe(1);
    expect(renders.status).toBe(1);
  });
});
