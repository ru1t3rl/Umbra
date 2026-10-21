import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, render, screen } from "@testing-library/react";
import type { ReactElement } from "react";
import { connectEngineStream } from "./connection.ts";
import { EngineStreamProvider } from "./EngineStreamProvider.tsx";
import { useEntitySource, useRunStatus, useStreamStatus } from "./hooks.ts";
import { entity, frame, hello, rig, run } from "./testing.ts";

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

describe("hooks", () => {
  test("useStreamStatus follows the connection", () => {
    const { r } = setup();
    expect(screen.getByTestId("status").textContent).toBe("open");
    act(() => r.socket().drop());
    expect(screen.getByTestId("status").textContent).toBe("reconnecting");
  });
  test("useRunStatus re-renders only on run changes", () => {
    const { flush, renders, connection } = setup();
    flush(frame(0, 1, [entity(1)]));
    expect(screen.getByTestId("run").textContent).toBe("running@1");
    const runRenders = renders.run;
    // Same run, different entities: no re-render of the run consumer.
    flush(frame(1, 1, [entity(1), entity(2)], { cause: "control" }));
    expect(renders.run).toBe(runRenders);
    // Errors and ignored messages do not touch it either.
    flush({ type: "error", code: "invalid_message", message: "m" }, { type: "mystery" });
    expect(renders.run).toBe(runRenders);
    expect(connection.getState().errors).toBe(1);
    // A real run change does.
    flush(frame(2, 2, [], { run: run(2) }));
    expect(screen.getByTestId("run").textContent).toBe("running@2");
    expect(renders.run).toBe(runRenders + 1);
  });
  test("useEntitySource is stable and delivers frames without rendering", () => {
    const r = rig();
    const connection = connectEngineStream(r.options);
    r.socket().receive(hello());
    const seen: unknown[] = [];
    let renders = 0;
    function Probe(): ReactElement {
      renders += 1;
      const source = useEntitySource();
      return <p>{source === connection.source ? "same" : "different"}</p>;
    }
    render(
      <EngineStreamProvider connection={connection}>
        <Probe />
      </EngineStreamProvider>,
    );
    const off = connection.source.subscribe((f) => seen.push(f.tick));
    act(() => {
      r.socket().receive(frame(0, 1, [entity(1)]));
      r.clock.paint();
    });
    off();
    expect(screen.getByText("same")).toBeTruthy();
    expect(seen).toEqual([1]);
    expect(renders).toBe(1);
  });
  test("a hook outside the provider throws a clear error", () => {
    function Lonely(): ReactElement {
      return <p>{useStreamStatus()}</p>;
    }
    const spy = console.error;
    console.error = () => {};
    try {
      expect(() => render(<Lonely />)).toThrow("EngineStreamProvider is missing");
    } finally {
      console.error = spy;
    }
  });
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
