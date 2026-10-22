// WI-5 review: connection records (connect_failed, disconnected, server_error, closed once), staged bound,
// flush scheduling and run-record triggers.
import { describe, expect, test } from "bun:test";
import { connectEngineStream } from "./connection.ts";
import { entity, frame, hello, rig, run } from "./testing.ts";
import type { JournalRecord } from "./types.ts";

const kinds = (c: { journal: { snapshot(): readonly JournalRecord[] } }, source?: string) =>
  c.journal
    .snapshot()
    .filter((x) => source === undefined || x.source === source)
    .map((x) => x.kind);

describe("connection records", () => {
  test("a socket closed before hello gives no disconnected; repeated failed retries give one connect_failed", () => {
    // Mutation caught: disconnected on every failed retry (the log fills with "connection lost" without an engine).
    const r = rig();
    const c = connectEngineStream(r.options);
    for (let i = 0; i < 4; i++) {
      r.socket().drop();
      r.clock.paint();
      r.clock.fireTimer();
    }
    r.clock.paint();
    expect(kinds(c)).toEqual(["connect_failed"]);
    expect(c.journal.snapshot()[0]?.level).toBe("warning");
  });
  test("after an open connection is lost: disconnected, then one connect_failed for the failed retries", () => {
    // Mutation caught: connect_failed never re-armed after hello.
    const r = rig();
    const c = connectEngineStream(r.options);
    r.socket().receive(hello());
    r.socket().drop();
    for (let i = 0; i < 3; i++) {
      r.clock.fireTimer();
      r.socket().drop();
    }
    r.clock.paint();
    expect(kinds(c)).toEqual(["connected", "disconnected", "connect_failed"]);
  });
  test("two outages after two successful opens give exactly two connect_failed records", () => {
    // Mutation caught: failureNoted not re-armed by hello (the second outage would log nothing).
    const r = rig();
    const c = connectEngineStream(r.options);
    for (let i = 0; i < 2; i++) {
      r.socket().receive(hello());
      r.socket().drop();
      r.clock.fireTimer();
      r.socket().drop(); // the retry fails before hello
      r.clock.fireTimer();
    }
    r.clock.paint();
    expect(kinds(c).filter((k) => k === "connect_failed")).toHaveLength(2);
  });
  test("a socket that cannot be created gives one connect_failed", () => {
    // Mutation caught: connect_failed only on close events.
    let calls = 0;
    const r = rig({
      createSocket: () => {
        calls += 1;
        throw new Error("blocked");
      },
    });
    const c = connectEngineStream(r.options);
    r.clock.fireTimer();
    r.clock.fireTimer();
    r.clock.paint();
    expect(calls).toBe(3);
    expect(kinds(c)).toEqual(["connect_failed"]);
  });
  test("server error messages are journaled as warnings with the code, and still counted", () => {
    // Mutation caught: counter only.
    const r = rig();
    const c = connectEngineStream(r.options);
    r.socket().receive(hello());
    r.socket().receive({ type: "error", code: "bad_filter", message: "no such kind" });
    r.clock.paint();
    const rec = c.journal.snapshot().find((x) => x.kind === "server_error");
    expect(rec).toMatchObject({
      source: "connection",
      level: "warning",
      message: "bad_filter: no such kind",
      detail: { code: "bad_filter", message: "no such kind" },
    });
    expect(c.getState().errors).toBe(1);
  });
  test("repeated close() journals closed once, also after an incompatible hello", () => {
    // Mutation caught: closed record per call; closed record suppressed by the incompatible flag.
    const a = rig();
    const ca = connectEngineStream(a.options);
    a.socket().receive(hello());
    ca.close();
    ca.close();
    expect(kinds(ca).filter((k) => k === "closed")).toHaveLength(1);
    const b = rig();
    const cb = connectEngineStream(b.options);
    b.socket().receive(hello("umbra.stream.v2"));
    cb.close();
    cb.close();
    expect(kinds(cb).filter((k) => k === "closed")).toHaveLength(1);
  });
});

describe("flush scheduling", () => {
  test("a pending frame then a disconnect share one scheduled flush; close cancels exactly once", () => {
    // Mutation caught: note() always scheduling a second frame.
    const r = rig();
    const c = connectEngineStream(r.options);
    r.socket().receive(hello());
    r.clock.paint();
    r.socket().receive(frame(0, 1, [entity(1)]));
    r.socket().drop();
    expect(r.clock.frames).toHaveLength(1);
    c.close();
    expect(r.clock.cancelledFrames).toBe(1);
  });
});

describe("run records", () => {
  test("stopped(command) then stopped(plugin_fault) gives a second, error-level record", () => {
    // Mutation caught: ignoring a stop_reason change while the state is the same.
    const r = rig();
    const c = connectEngineStream(r.options);
    r.socket().receive(hello());
    r.socket().receive(
      frame(0, 1, [], { run: run(1, { state: "stopped", stop_reason: "command" }) }),
    );
    r.socket().receive(
      frame(1, 2, [], { run: run(2, { state: "stopped", stop_reason: "plugin_fault" }) }),
    );
    r.clock.paint();
    const stops = c.journal.snapshot().filter((x) => x.kind === "stopped");
    expect(stops.map((x) => x.level)).toEqual(["info", "error"]);
  });
  test("a changed fault on the same state adds a record; an equal fault does not", () => {
    // Mutation caught: faults compared by identity (a record per frame) or not at all.
    const r = rig();
    const c = connectEngineStream(r.options);
    r.socket().receive(hello());
    const f = (message: string) => ({ call: "u", capability: "m", plugin: "p", message });
    r.socket().receive(frame(0, 1, [], { run: run(1, { fault: f("a") }) }));
    r.socket().receive(frame(1, 2, [], { run: run(2, { fault: f("a") }) }));
    r.socket().receive(frame(2, 3, [], { run: run(3, { fault: f("b") }) }));
    r.clock.paint();
    const runs = c.journal.snapshot().filter((x) => x.source === "run" && x.kind === "running");
    expect(runs.map((x) => x.message.includes("b"))).toEqual([false, true]);
  });
});
