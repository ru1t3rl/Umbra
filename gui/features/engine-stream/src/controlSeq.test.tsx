// WI-10 / ASM-938 (written before the store change): `controlSeq` is a monotonic count of decoded control
// frames, kept next to the coalesced frame so "latest wins" cannot lose a command. It is published with the
// flush (one store update per flush), never reset (not by a new session, not by a reconnect), and exposed by
// `useControlSeq()`, which re-renders only when the count changes.
import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, render, screen } from "@testing-library/react";
import type { ReactElement } from "react";
import { EngineStreamProvider } from "./components/EngineStreamProvider/EngineStreamProvider.tsx";
import { connectEngineStream } from "./connection.ts";
import { useControlSeq } from "./hooks/useControlSeq.ts";
import { entity, frame, hello, rig } from "./testing.ts";

afterEach(cleanup);

function open() {
  const r = rig();
  const connection = connectEngineStream(r.options);
  r.socket().receive(hello());
  const seq = () => (connection.getState() as { controlSeq?: number }).controlSeq;
  const flush = (...messages: unknown[]) => {
    for (const m of messages) r.socket().receive(m);
    r.clock.paint();
  };
  return { r, connection, seq, flush };
}
const control = (n: number, extra: Record<string, unknown> = {}) =>
  frame(n, n, [entity(1)], { cause: "control", ...extra });
const tick = (n: number) => frame(n, n, [entity(1)]);

describe("controlSeq in the store", () => {
  test("starts at 0", () => {
    expect(open().seq()).toBe(0);
  });
  test("a control frame increments it", () => {
    const { seq, flush } = open();
    flush(control(0));
    expect(seq()).toBe(1);
    flush(control(1));
    expect(seq()).toBe(2);
  });
  test("tick frames, unknown causes, errors and ignored messages do not", () => {
    const { seq, flush } = open();
    flush(tick(0), frame(1, 1, [], { cause: "replay" }), { type: "mystery" });
    flush({ type: "error", code: "invalid_message", message: "m" });
    expect(seq()).toBe(0);
  });
  test("control frames coalesced into one flush each still count", () => {
    const { seq, flush } = open();
    flush(control(0), control(1), tick(2), control(3));
    expect(seq()).toBe(3);
  });
  test("a control frame replaced by a tick frame before the flush still counts", () => {
    const { seq, flush, connection } = open();
    flush(control(0), tick(1));
    expect(seq()).toBe(1);
    expect(connection.getState().frame?.seq).toBe(1); // the tick frame won, the command was not lost
  });
  test("is not reset by a new session or by a reconnect", () => {
    const { r, seq, flush } = open();
    flush(control(0), control(1));
    flush(control(2, { session: 2 }));
    expect(seq()).toBe(3);
    r.socket().drop();
    r.clock.fireTimer();
    r.socket().receive(hello());
    flush(control(0));
    expect(seq()).toBe(4);
  });
  test("a flush with no control frame publishes the same count (no spurious change)", () => {
    const { seq, flush } = open();
    flush(control(0));
    flush(tick(1), tick(2));
    expect(seq()).toBe(1);
  });
});

describe("useControlSeq", () => {
  test("returns the count and re-renders only when it changes", () => {
    const { r, connection, flush } = open();
    let renders = 0;
    function Probe(): ReactElement {
      renders += 1;
      return <p data-testid="seq">{useControlSeq()}</p>;
    }
    render(
      <EngineStreamProvider connection={connection}>
        <Probe />
      </EngineStreamProvider>,
    );
    expect(screen.getByTestId("seq").textContent).toBe("0");
    const before = renders;
    act(() => flush(tick(0), tick(1)));
    expect(renders).toBe(before);
    act(() => flush(control(2), control(3)));
    expect(screen.getByTestId("seq").textContent).toBe("2");
    expect(renders).toBe(before + 1);
    void r;
  });
});

describe("the public API", () => {
  test("useControlSeq is exported from the package", async () => {
    const api = await import("./index.ts");
    expect(typeof (api as Record<string, unknown>).useControlSeq).toBe("function");
  });
});
