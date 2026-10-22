// WI-5 / ADR 0071 / plan "Performance plan" 3. Assumed: useEntity(id: number | undefined): EntitySummary | undefined,
// a narrow selector over the newest frame (binary search, ids ascend). It re-renders only when THAT entity's
// content changes (shallow field equality, route compared by len/next), and keeps the previous object identity
// when the content is equal, although every frame decodes to fresh objects.
import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, render, screen } from "@testing-library/react";
import type { ReactElement } from "react";
import { EngineStreamProvider } from "../components/EngineStreamProvider/EngineStreamProvider.tsx";
import { connectEngineStream, type EngineStreamConnection } from "../connection.ts";
import { entity, frame, hello, rig } from "../testing.ts";
import type { EntityFrame, EntitySummary } from "../types.ts";
import { useEntity } from "./useEntity.ts";

afterEach(cleanup);

function setup(id: number | undefined) {
  const r = rig();
  const connection = connectEngineStream(r.options);
  r.socket().receive(hello());
  const seen: (EntitySummary | undefined)[] = [];
  function Probe({ of }: { of: number | undefined }): ReactElement {
    const e = useEntity(of);
    seen.push(e);
    return <p data-testid="e">{e ? `${e.id}:${e.lat_deg}:${e.speed_mps}` : "none"}</p>;
  }
  const ui = render(
    <EngineStreamProvider connection={connection}>
      <Probe of={id} />
    </EngineStreamProvider>,
  );
  const rerenderWith = (of: number | undefined) =>
    ui.rerender(
      <EngineStreamProvider connection={connection}>
        <Probe of={of} />
      </EngineStreamProvider>,
    );
  const flush = (...messages: unknown[]) =>
    act(() => {
      for (const m of messages) r.socket().receive(m);
      r.clock.paint();
    });
  return { r, connection, seen, flush, rerenderWith };
}

describe("useEntity", () => {
  test("is undefined before the first frame and for an absent id", () => {
    // Mutation caught: returning a placeholder object or throwing on no frame.
    const { flush } = setup(99);
    expect(screen.getByTestId("e").textContent).toBe("none");
    flush(frame(0, 1, [entity(1), entity(2)]));
    expect(screen.getByTestId("e").textContent).toBe("none");
  });
  test("returns the entity with that id from the newest frame", () => {
    // Mutation caught: returning the first/nearest entity; off-by-one in the search.
    const { flush } = setup(3);
    flush(frame(0, 1, [entity(1), entity(2), entity(3, { speed_mps: 9 }), entity(5)]));
    expect(screen.getByTestId("e").textContent).toBe(`3:${10 + 3 / 1e5}:9`);
  });
  test("finds every id of an odd and an even sized frame, including the first and the last", () => {
    // Mutation caught: a binary search that misses the ends.
    for (const n of [1, 2, 7, 8]) {
      const ids = Array.from({ length: n }, (_, i) => (i + 1) * 3);
      for (const id of [ids[0], ids[n - 1]]) {
        cleanup();
        const { flush } = setup(id);
        flush(
          frame(
            0,
            1,
            ids.map((i) => entity(i)),
          ),
        );
        expect(screen.getByTestId("e").textContent?.startsWith(`${id}:`)).toBe(true);
      }
    }
  });
  test("an ids-in-between lookup does not match a neighbour", () => {
    // Mutation caught: lower_bound without the equality check.
    const { flush } = setup(4);
    flush(frame(0, 1, [entity(3), entity(5)]));
    expect(screen.getByTestId("e").textContent).toBe("none");
  });
  test("a change of that entity re-renders with the new value", () => {
    // Mutation caught: a selector that never updates (stale entity).
    const { flush, seen } = setup(1);
    flush(frame(0, 1, [entity(1, { speed_mps: 5 })]));
    flush(frame(1, 2, [entity(1, { speed_mps: 7 })]));
    expect(screen.getByTestId("e").textContent).toBe(`1:${10 + 1 / 1e5}:7`);
    expect(seen.at(-1)?.speed_mps).toBe(7);
  });
  test("a change of another entity does not re-render and keeps the same object", () => {
    // Mutation caught: selecting the frame or the entities array (re-render per flush).
    const { flush, seen } = setup(1);
    flush(frame(0, 1, [entity(1), entity(2)]));
    const renders = seen.length;
    const first = seen.at(-1);
    for (let i = 1; i <= 5; i++) {
      flush(frame(i, 1 + i, [entity(1), entity(2, { lat_deg: 50 + i }), entity(3)]));
    }
    expect(seen.length).toBe(renders);
    expect(seen.at(-1)).toBe(first);
  });
  test("an equal entity in a fresh frame keeps identity and does not re-render, also with a route", () => {
    // Mutation caught: reference equality (every decoded frame has new objects); ignoring route in the comparison.
    const { flush, seen } = setup(1);
    flush(frame(0, 1, [entity(1, { route: { len: 3, next: 1 } })]));
    const renders = seen.length;
    flush(frame(1, 2, [entity(1, { route: { len: 3, next: 1 } })]));
    expect(seen.length).toBe(renders);
    flush(frame(2, 3, [entity(1, { route: { len: 3, next: 2 } })]));
    expect(seen.length).toBe(renders + 1);
    expect(seen.at(-1)?.route).toEqual({ len: 3, next: 2 });
  });
  test("every field change counts: name, status and ownership", () => {
    // Mutation caught: comparing only position fields.
    const { flush, seen } = setup(1);
    flush(frame(0, 1, [entity(1, { name: "a", status: "ok" })]));
    let renders = seen.length;
    for (const [i, change] of [
      { name: "b" },
      { status: "hit" },
      { ownership: "remote" },
    ].entries()) {
      flush(frame(i + 1, i + 2, [entity(1, { name: "a", status: "ok", ...change })]));
      expect(seen.length).toBe(renders + 1);
      renders = seen.length;
      flush(frame(i + 10, i + 20, [entity(1, { name: "a", status: "ok" })])); // back
      renders = seen.length;
    }
  });
  test("becomes undefined when the entity is removed or the session changes", () => {
    // Mutation caught: keeping the last known entity after removal.
    const { flush } = setup(2);
    flush(frame(0, 1, [entity(1), entity(2)]));
    expect(screen.getByTestId("e").textContent).not.toBe("none");
    flush(frame(1, 2, [entity(1)]));
    expect(screen.getByTestId("e").textContent).toBe("none");
    flush(frame(2, 3, [entity(1), entity(2)]));
    expect(screen.getByTestId("e").textContent).not.toBe("none");
    flush(frame(0, 1, [entity(1)], { session: 2 }));
    expect(screen.getByTestId("e").textContent).toBe("none");
  });
  test("a changed id argument selects the other entity; undefined selects nothing", () => {
    // Mutation caught: a selector closed over the first id.
    const { flush, rerenderWith } = setup(1);
    flush(frame(0, 1, [entity(1), entity(2, { speed_mps: 8 })]));
    act(() => rerenderWith(2));
    expect(screen.getByTestId("e").textContent).toBe(`2:${10 + 2 / 1e5}:8`);
    act(() => rerenderWith(undefined));
    expect(screen.getByTestId("e").textContent).toBe("none");
  });
  test("looks up by binary search: a 10,000-entity frame costs O(log n) index reads", () => {
    // Mutation caught: Array.find / linear scan (would read thousands of elements).
    const r = rig();
    const real = connectEngineStream(r.options);
    const list: EntitySummary[] = Array.from({ length: 10_000 }, (_, i) => ({
      ...(entity(i + 1) as Omit<EntitySummary, "ownership">),
      ownership: "owned",
    }));
    let reads = 0;
    const entities = new Proxy(list, {
      get(target, prop, receiver) {
        if (typeof prop === "string" && /^\d+$/.test(prop)) reads += 1;
        return Reflect.get(target, prop, receiver);
      },
    });
    const f: EntityFrame = { session: 1, tick: 1, time_s: 0, seq: 0, dropped: 0, entities };
    const connection: EngineStreamConnection = {
      ...real,
      source: { getFrame: () => f, subscribe: () => () => {} },
    };
    function Probe(): ReactElement {
      return <p>{useEntity(7_777)?.id}</p>;
    }
    render(
      <EngineStreamProvider connection={connection}>
        <Probe />
      </EngineStreamProvider>,
    );
    expect(screen.getByText("7777")).toBeTruthy();
    expect(reads).toBeGreaterThan(0);
    expect(reads).toBeLessThan(200);
  });
});
