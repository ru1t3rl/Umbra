import { describe, expect, test } from "bun:test";
import { entity, frame, hello, run } from "../testing.ts";
import { decodeJson } from "./decode.ts";

const dec = (m: unknown) => decodeJson(typeof m === "string" ? m : JSON.stringify(m));

describe("decodeJson", () => {
  test("hello", () => {
    const d = dec(hello());
    expect(d.kind).toBe("hello");
    if (d.kind === "hello") expect(d.hello.settings.max_rate_hz).toBe(60);
  });
  test("an unknown message type is ignored, never cast", () => {
    const d = dec({ type: "telemetry", payload: 1 });
    expect(d).toEqual({ kind: "ignored", reason: "unknown message type" });
  });
  test("bad JSON, non-objects and a message without type are ignored", () => {
    for (const raw of ["{nope", "[]", "3", "null", '{"id":1}']) {
      expect(dec(raw).kind).toBe("ignored");
    }
  });
  test("unknown fields are dropped silently", () => {
    const d = dec(frame(0, 1, [entity(1, { future_field: { a: 1 } })], { future: true }));
    expect(d.kind).toBe("frame");
  });
  test("a frame with an unknown cause or ownership is handled", () => {
    const d = dec(
      frame(0, 1, [entity(1, { ownership: "shared" }), entity(2)], { cause: "replay" }),
    );
    expect(d.kind).toBe("frame");
    if (d.kind !== "frame") return;
    expect(d.frame.entities.map((e) => e.ownership)).toEqual(["unknown", "owned"]);
  });
  test("unknown ownership is unknown, also when missing or mistyped", () => {
    const missing = { ...entity(2), ownership: undefined };
    const d = dec(
      frame(0, 1, [entity(1, { ownership: "shared" }), missing, entity(3, { ownership: 7 })]),
    );
    if (d.kind !== "frame") throw new Error("expected a frame");
    expect(d.frame.entities.map((e) => e.ownership)).toEqual(["unknown", "unknown", "unknown"]);
    expect(d.skipped).toBe(0);
  });
  test("a malformed route becomes null, a good one is kept", () => {
    const d = dec(
      frame(0, 1, [
        entity(1, { route: { len: 3, next: 1 } }),
        entity(2, { route: { len: "3" } }),
        entity(3, { route: "north" }),
        entity(4, { route: null }),
        entity(5),
      ]),
    );
    if (d.kind !== "frame") throw new Error("expected a frame");
    expect(d.frame.entities.map((e) => e.route)).toEqual([
      { len: 3, next: 1 },
      null,
      null,
      null,
      undefined,
    ]);
  });
  test("an entity with a missing or mistyped required field is skipped and counted, the rest kept", () => {
    const bad = { ...entity(2), lat_deg: "north" };
    const d = dec(frame(0, 1, [entity(1), bad, null, 5, entity(3)]));
    if (d.kind !== "frame") throw new Error("expected a frame");
    expect(d.frame.entities.map((e) => e.id)).toEqual([1, 3]);
    expect(d.skipped).toBe(3);
  });
  test("a frame without its required numbers is ignored", () => {
    expect(dec({ ...frame(0, 1), tick: "1" }).kind).toBe("ignored");
    expect(dec({ ...frame(0, 1), entities: {} }).kind).toBe("ignored");
  });
  test("the entities array is frozen", () => {
    const d = dec(frame(0, 1, [entity(1)]));
    if (d.kind !== "frame") throw new Error("expected a frame");
    expect(Object.isFrozen(d.frame.entities)).toBe(true);
  });
  test("unknown state yields state unknown and keeps tick and time", () => {
    const d = dec(frame(0, 1, [entity(1)], { run: run(7, { state: "hibernating" }) }));
    if (d.kind !== "frame") throw new Error("expected a frame");
    expect(d.run?.state).toBe("unknown");
    expect(d.run?.tick).toBe(7);
    expect(d.run?.time_s).toBeCloseTo(7 / 30);
    expect(d.frame.entities).toHaveLength(1);
  });
  test("unknown clock mode keeps tick/time", () => {
    for (const clock of [{ mode: "warp" }, { mode: "scaled" }, "fast", null]) {
      const d = dec(frame(0, 1, [], { run: run(9, { clock }) }));
      if (d.kind !== "frame") throw new Error("expected a frame");
      expect(d.run?.clock).toEqual({ mode: "unknown" });
      expect(d.run?.state).toBe("running");
      expect(d.run?.tick).toBe(9);
      expect(d.run?.time_s).toBeCloseTo(9 / 30);
    }
  });
  test("a run without a session is not a run status", () => {
    const d = dec(frame(0, 1, [], { run: { ...run(1), session: undefined } }));
    if (d.kind !== "frame") throw new Error("expected a frame");
    expect(d.run).toBeUndefined();
  });
  test("an unknown stop_reason becomes unknown, a known one is kept, null stays absent", () => {
    const unknown = dec(
      frame(0, 1, [], { run: run(1, { state: "stopped", stop_reason: "meteor" }) }),
    );
    const known = dec(
      frame(0, 1, [], { run: run(1, { state: "stopped", stop_reason: "duration" }) }),
    );
    if (unknown.kind !== "frame" || known.kind !== "frame") throw new Error("expected frames");
    expect(unknown.run?.state).toBe("stopped");
    expect(unknown.run?.stop_reason).toBe("unknown");
    const none = dec(frame(0, 1, [], { run: run(1, { state: "stopped", stop_reason: null }) }));
    if (none.kind !== "frame") throw new Error("expected frame");
    expect(none.run?.stop_reason).toBeUndefined();
    expect(known.run?.stop_reason).toBe("duration");
  });
  test("a scaled clock keeps its scale; null optional run fields are tolerated", () => {
    const d = dec(
      frame(0, 1, [], {
        run: run(1, { clock: { mode: "scaled", scale: 4 }, fault: null, tick_rate_hz: null }),
      }),
    );
    if (d.kind !== "frame") throw new Error("expected a frame");
    expect(d.run?.clock).toEqual({ mode: "scaled", scale: 4 });
    expect(d.run?.tick_rate_hz).toBeUndefined();
  });
  test("an error message with an unknown code is still an error", () => {
    expect(dec({ type: "error", code: "from_the_future", message: "m" })).toEqual({
      kind: "error",
      code: "from_the_future",
      message: "m",
    });
  });
});
