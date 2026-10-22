// WI-5 / ADR 0071 (amends ADR 0045 / ASM-300 for events): the connection feeds a lossless journal at decode
// time, before coalescing. Assumed API (ASM-895):
//   EngineStreamOptions.journalCapacity?: number (default 5000), .now?: () => number (default Date.now)
//   EngineStreamConnection.journal: JournalSource
//   Per frame, in this order: session record (source "run", kind "session") when frame.session changed (also the
//   first), run records (source "run", kind = state, only when state or stop_reason changed), one record per event
//   (source "event", level "info", kind verbatim, detail = the event as received), then loss records (source
//   "event", level "warning", kinds "events_truncated" and "frames_dropped" with detail { dropped }).
//   Connection records (source "connection"): "connected" (first hello), "reconnected" (later hello), "disconnected"
//   (warning, socket lost), "closed" (close()), "incompatible" (error). Records become visible at the next
//   flush; close() publishes synchronously.
import { describe, expect, test } from "bun:test";
import { connectEngineStream } from "./connection.ts";
import { entity, ev, frame, hello, rig, run } from "./testing.ts";
import type { JournalRecord } from "./types.ts";

function open(extra = {}) {
  const r = rig(extra);
  const c = connectEngineStream(r.options);
  r.socket().receive(hello());
  r.clock.paint();
  const recs = (): readonly JournalRecord[] => c.journal.snapshot();
  const only = (source: JournalRecord["source"]) => recs().filter((x) => x.source === source);
  return { r, c, recs, only };
}

describe("events", () => {
  test("events of coalesced frames are all journaled in order", () => {
    // Mutation caught: journaling the pending (latest) frame at flush instead of every decoded frame.
    const { r, c, only } = open();
    for (let i = 0; i < 3; i++) {
      r.socket().receive(
        frame(i, i + 1, [entity(1)], {
          events: [
            ev("fire", i + 1, { entity_id: 1, weapon: `w${i}a` }),
            ev("custom", i + 1, { entity_id: 1 }),
          ],
        }),
      );
    }
    r.clock.paint();
    expect(c.getState().frame?.tick).toBe(3); // the map only sees the newest frame
    expect(only("event").map((x) => [x.kind, x.tick])).toEqual([
      ["fire", 1],
      ["custom", 1],
      ["fire", 2],
      ["custom", 2],
      ["fire", 3],
      ["custom", 3],
    ]);
  });
  test("an event record carries kind, tick, entity, session, level and the event as detail", () => {
    // Mutation caught: dropping entity_id/session/detail, wrong source or level.
    const { r, only } = open();
    const e = ev("fire", 4, { entity_id: 7, target_id: 8, weapon: "gun" });
    r.socket().receive(frame(0, 4, [], { session: 2, events: [e] }));
    r.clock.paint();
    const rec = only("event")[0];
    expect(rec).toMatchObject({
      source: "event",
      level: "info",
      kind: "fire",
      tick: 4,
      entity_id: 7,
      session: 2,
      detail: e,
    });
    expect(rec?.message.length).toBeGreaterThan(0);
  });
  test("unknown event kinds are journaled verbatim", () => {
    // Mutation caught: mapping unknown kinds to "unknown" or dropping them.
    const { r, only } = open();
    const e = ev("jamming_started", 2, { power_w: 5 });
    r.socket().receive(frame(0, 2, [], { events: [e] }));
    r.clock.paint();
    expect(only("event")[0]).toMatchObject({ kind: "jamming_started", detail: e });
  });
  test("events_truncated adds one loss record", () => {
    // Mutation caught: no record; one record per event; wrong level.
    const { r, only } = open();
    r.socket().receive(
      frame(0, 1, [], { events: [ev("fire", 1), ev("fire", 1)], events_truncated: true }),
    );
    r.clock.paint();
    const loss = only("event").filter((x) => x.kind === "events_truncated");
    expect(loss).toHaveLength(1);
    expect(loss[0]?.level).toBe("warning");
    expect(only("event").filter((x) => x.kind === "fire")).toHaveLength(2);
  });
  test("dropped > 0 adds one loss record with the count, dropped 0 adds none", () => {
    // Mutation caught: `>= 0`, count lost, record per flush instead of per frame.
    const { r, only } = open();
    r.socket().receive(frame(0, 1, [], { dropped: 0 }));
    r.socket().receive(frame(1, 2, [], { dropped: 4 }));
    r.clock.paint();
    const loss = only("event").filter((x) => x.kind === "frames_dropped");
    expect(loss).toHaveLength(1);
    expect(loss[0]).toMatchObject({ level: "warning", detail: { dropped: 4 } });
    expect(loss[0]?.message).toContain("4");
  });
  test("within one frame the order is session, run, events, then loss records", () => {
    // Mutation caught: loss records before the events they describe.
    const { r, recs } = open();
    r.socket().receive(
      frame(0, 1, [], {
        events: [ev("fire", 1)],
        events_truncated: true,
        dropped: 2,
        run: run(1, { state: "running" }),
      }),
    );
    r.clock.paint();
    expect(
      recs()
        .filter((x) => x.source !== "connection")
        .map((x) => `${x.source}:${x.kind}`),
    ).toEqual([
      "run:session",
      "run:running",
      "event:fire",
      "event:events_truncated",
      "event:frames_dropped",
    ]);
  });
  test("events of a stale socket are not journaled", () => {
    // Mutation caught: journaling before the `socket !== current` guard.
    const { r, only } = open();
    const old = r.socket();
    old.drop();
    r.clock.fireTimer();
    old.receive(frame(0, 1, [], { events: [ev("fire", 1)] }));
    r.clock.paint();
    expect(only("event")).toHaveLength(0);
  });
  test("events are not stored in the entity store (ASM-300 still holds for the store)", () => {
    // Mutation caught: leaking events into StreamState and re-rendering every store subscriber per event.
    const { r, c } = open();
    r.socket().receive(frame(0, 1, [], { events: [ev("fire", 1)] }));
    r.clock.paint();
    expect(JSON.stringify(c.getState())).not.toContain("fire");
  });
});

describe("run, session and connection records", () => {
  test("run state transitions are journaled once each, also within one coalesced flush", () => {
    // Mutation caught: a record per frame; journaling only the latest run of a flush.
    const { r, only } = open();
    r.socket().receive(frame(0, 1, [], { run: run(1, { state: "running" }) }));
    r.socket().receive(frame(1, 2, [], { run: run(2, { state: "running" }) }));
    r.socket().receive(frame(2, 3, [], { run: run(3, { state: "paused" }) }));
    r.clock.paint();
    expect(
      only("run")
        .filter((x) => x.kind !== "session")
        .map((x) => [x.kind, x.level]),
    ).toEqual([
      ["running", "info"],
      ["paused", "info"],
    ]);
  });
  test("a stop with a plugin fault is an error record that names the fault", () => {
    // Mutation caught: level info; message without the fault text.
    const { r, only } = open();
    const fault = { call: "update", capability: "motion", plugin: "wander", message: "panicked" };
    r.socket().receive(
      frame(0, 9, [], { run: run(9, { state: "stopped", stop_reason: "plugin_fault", fault }) }),
    );
    r.clock.paint();
    const rec = only("run").find((x) => x.kind === "stopped");
    expect(rec?.level).toBe("error");
    expect(rec?.message).toContain("panicked");
  });
  test("stop reasons map to levels: network_fault and error are errors, duration and command are info", () => {
    // Mutation caught: a table with one wrong entry.
    const levels: Record<string, string> = {};
    for (const reason of ["network_fault", "error", "duration", "command"]) {
      const { r, only } = open();
      r.socket().receive(
        frame(0, 1, [], { run: run(1, { state: "stopped", stop_reason: reason }) }),
      );
      r.clock.paint();
      levels[reason] = only("run").find((x) => x.kind === "stopped")?.level ?? "missing";
    }
    expect(levels).toEqual({
      network_fault: "error",
      error: "error",
      duration: "info",
      command: "info",
    });
  });
  test("an unknown stop reason is a warning and a fault on a running run is a warning", () => {
    // Mutation caught: treating unknown as info (a newer engine's failure would look harmless).
    const a = open();
    a.r
      .socket()
      .receive(frame(0, 1, [], { run: run(1, { state: "stopped", stop_reason: "meltdown" }) }));
    a.r.clock.paint();
    expect(a.only("run").find((x) => x.kind === "stopped")?.level).toBe("warning");
    const b = open();
    const fault = { call: "c", capability: "k", plugin: "p", message: "m" };
    b.r.socket().receive(frame(0, 1, [], { run: run(1, { state: "running", fault }) }));
    b.r.clock.paint();
    expect(b.only("run").some((x) => x.level === "warning")).toBe(true);
  });
  test("a new session adds a session record and keeps earlier records", () => {
    // Mutation caught: clearing the journal on a session change (ADR 0071: the log spans sessions).
    const { r, recs, only } = open();
    r.socket().receive(frame(0, 1, [], { session: 1, events: [ev("fire", 1)] }));
    r.clock.paint();
    const before = recs().length;
    r.socket().receive(frame(1, 1, [], { session: 2, run: run(1, { session: 2 }) }));
    r.clock.paint();
    expect(
      only("run")
        .filter((x) => x.kind === "session")
        .map((x) => x.session),
    ).toEqual([1, 2]);
    expect(recs().length).toBeGreaterThan(before);
    expect(only("event").map((x) => x.kind)).toEqual(["fire"]);
  });
  test("a frame of the same session adds no session record", () => {
    // Mutation caught: a session record per frame.
    const { r, only } = open();
    for (let i = 0; i < 3; i++) r.socket().receive(frame(i, i + 1));
    r.clock.paint();
    expect(only("run").filter((x) => x.kind === "session")).toHaveLength(1);
  });
  test("connection open, loss, reconnect and close are journaled in order with levels", () => {
    // Mutation caught: no record on reconnect; the same kind for first and later hello; wrong level on loss.
    const { r, c, only } = open();
    r.socket().drop();
    r.clock.paint();
    r.clock.fireTimer();
    r.socket().receive(hello());
    r.clock.paint();
    c.close();
    expect(only("connection").map((x) => [x.kind, x.level])).toEqual([
      ["connected", "info"],
      ["disconnected", "warning"],
      ["reconnected", "info"],
      ["closed", "info"],
    ]);
  });
  test("an incompatible protocol is journaled as an error", () => {
    // Mutation caught: silent incompatibility (the log would show nothing for a dead connection).
    const r = rig();
    const c = connectEngineStream(r.options);
    r.socket().receive(hello("umbra.stream.v2"));
    r.clock.paint();
    expect(c.journal.snapshot().find((x) => x.kind === "incompatible")?.level).toBe("error");
  });
  test("seq keeps increasing across reconnects and sessions", () => {
    // Mutation caught: resetting the counter or the ring on reconnect.
    const { r, c } = open();
    r.socket().receive(frame(0, 1, [], { events: [ev("fire", 1)] }));
    r.clock.paint();
    r.socket().drop();
    r.clock.fireTimer();
    r.socket().receive(hello());
    r.socket().receive(frame(0, 1, [], { session: 2, events: [ev("fire", 1)] }));
    r.clock.paint();
    const seqs = c.journal.snapshot().map((x) => x.seq);
    expect(seqs).toEqual(seqs.map((_, i) => i + 1));
    expect(seqs.length).toBeGreaterThan(6);
  });
});

describe("ring, clock and notification", () => {
  test("the ring keeps the newest capacity records and counts the evicted", () => {
    // Mutation caught: ignoring journalCapacity; evicting from the wrong end.
    const { r, c } = open({ journalCapacity: 5 });
    for (let i = 0; i < 20; i++)
      r.socket().receive(frame(0, 1, [], { events: [ev("fire", 1, { weapon: `w${i}` })] }));
    r.clock.paint();
    const snap = c.journal.snapshot();
    expect(c.journal.capacity).toBe(5);
    expect(snap).toHaveLength(5);
    expect(snap.map((x) => (x.detail as { weapon: string }).weapon)).toEqual([
      "w15",
      "w16",
      "w17",
      "w18",
      "w19",
    ]);
    expect(c.journal.evicted()).toBeGreaterThan(15);
  });
  test("the default capacity is 5,000", () => {
    // Mutation caught: a different default (ASM-824).
    expect(open().c.journal.capacity).toBe(5000);
  });
  test("received_at comes from the injected clock", () => {
    // Mutation caught: hard-wired Date.now.
    const { r, c } = open({ now: () => 4242 });
    r.socket().receive(frame(0, 1, [], { events: [ev("fire", 1)] }));
    r.clock.paint();
    expect(c.journal.snapshot().at(-1)?.received_at).toBe(4242);
  });
  test("journal subscribers are notified at most once per flush", () => {
    // Mutation caught: notify per frame, per record, or per flush even when nothing was added.
    const { r, c } = open();
    let n = 0;
    c.journal.subscribe(() => {
      n += 1;
    });
    for (let i = 0; i < 10; i++) {
      r.socket().receive(frame(i, i + 1, [], { events: [ev("fire", i + 1), ev("custom", i + 1)] }));
    }
    expect(n).toBe(0); // nothing before the animation frame
    r.clock.paint();
    expect(n).toBe(1);
    r.socket().receive(frame(10, 11, [], { run: run(11) })); // same session, same state, no events
    r.clock.paint();
    expect(n).toBe(1);
  });
  test("a hello and a frame in the same animation frame share one flush and one notification", () => {
    // Mutation caught: a second scheduled flush for connection records (also keeps the M5b flush-count tests valid).
    const r = rig();
    const c = connectEngineStream(r.options);
    let n = 0;
    c.journal.subscribe(() => {
      n += 1;
    });
    r.socket().receive(hello());
    r.socket().receive(frame(0, 1, [], { events: [ev("fire", 1)] }));
    expect(r.clock.frames).toHaveLength(1);
    r.clock.paint();
    expect(n).toBe(1);
  });
  test("close() publishes its record at once and cancels the pending flush exactly once", () => {
    // Mutation caught: close record only visible after a flush that was cancelled; double cancel.
    const { r, c } = open();
    let n = 0;
    c.journal.subscribe(() => {
      n += 1;
    });
    r.socket().receive(frame(0, 1, [], { events: [ev("fire", 1)] }));
    c.close();
    expect(r.clock.cancelledFrames).toBe(1);
    expect(c.journal.snapshot().at(-1)?.kind).toBe("closed");
    expect(n).toBe(1);
    const stale = c.journal.snapshot().length;
    r.socket().receive(frame(1, 2, [], { events: [ev("fire", 2)] }));
    r.clock.paint();
    expect(c.journal.snapshot()).toHaveLength(stale); // nothing is journaled after close
  });
});
