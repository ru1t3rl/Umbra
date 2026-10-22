// WI-7 (ASM-909): the raw word of an unknown stop reason, and network_fault change detection.
import { describe, expect, test } from "bun:test";
import { connectEngineStream } from "./connection.ts";
import { decodeJson } from "./model/decode.ts";
import { sameRun } from "./model/store.ts";
import { frame, hello, rig, run } from "./testing.ts";
import type { JournalRecord } from "./types.ts";

const runOf = (extra: Record<string, unknown>) => {
  const d = decodeJson(JSON.stringify(frame(0, 1, [], { run: run(1, extra) })));
  if (d.kind !== "frame" || !d.run) throw new Error("expected a frame with a run");
  return d.run;
};

describe("stop_reason_raw", () => {
  // Catches: the raw word lost (RunFaultAlert could only say "unknown").
  test("an unknown reason keeps its word", () => {
    const r = runOf({ state: "stopped", stop_reason: "meteor" });
    expect(r.stop_reason).toBe("unknown");
    expect(r.stop_reason_raw).toBe("meteor");
  });

  // Catches: a raw word attached to a known reason, or to a missing one.
  test("known and absent reasons have no raw word", () => {
    expect(runOf({ stop_reason: "error" }).stop_reason_raw).toBeUndefined();
    expect(runOf({}).stop_reason_raw).toBeUndefined();
    expect(runOf({ stop_reason: null }).stop_reason_raw).toBeUndefined();
  });

  // Catches: a non-string reason becoming a displayed word.
  test("a non-string reason is unknown without a word", () => {
    const r = runOf({ stop_reason: 7 });
    expect(r.stop_reason).toBe("unknown");
    expect(r.stop_reason_raw).toBeUndefined();
  });
});

describe("sameRun", () => {
  const a = runOf({
    state: "stopped",
    stop_reason: "network_fault",
    network_fault: { adapter: "a", reason: "r" },
  });
  // Catches: a changed network fault treated as no change (subscribers would not re-render).
  test("compares network_fault and the raw word by value", () => {
    expect(
      sameRun(
        a,
        runOf({
          state: "stopped",
          stop_reason: "network_fault",
          network_fault: { adapter: "a", reason: "r" },
        }),
      ),
    ).toBe(true);
    expect(
      sameRun(
        a,
        runOf({
          state: "stopped",
          stop_reason: "network_fault",
          network_fault: { adapter: "a", reason: "other" },
        }),
      ),
    ).toBe(false);
    expect(sameRun(a, runOf({ state: "stopped", stop_reason: "network_fault" }))).toBe(false);
    expect(sameRun(runOf({ stop_reason: "x" }), runOf({ stop_reason: "y" }))).toBe(false);
  });
});

describe("journal", () => {
  const kinds = (c: { journal: { snapshot(): readonly JournalRecord[] } }) =>
    c.journal.snapshot().filter((x) => x.source === "run");

  // Catches: a changed network fault on the same state making no run record (ASM-896).
  test("a changed network fault on the same state makes a run record", () => {
    const h = rig();
    const c = connectEngineStream(h.options);
    h.socket().receive(hello());
    const stopped = (reason: string) =>
      frame(0, 1, [], {
        run: run(1, {
          state: "stopped",
          stop_reason: "network_fault",
          network_fault: { adapter: "dis", reason },
        }),
      });
    h.socket().receive(stopped("one"));
    h.clock.paint();
    h.socket().receive(stopped("one"));
    h.clock.paint();
    h.socket().receive(stopped("two"));
    h.clock.paint();
    const runs = kinds(c).filter((x) => x.kind === "stopped");
    expect(runs).toHaveLength(2);
    expect(runs[1]?.message).toContain("dis: two");
    expect(runs[1]?.level).toBe("error");
  });

  // Catches: the journal saying "(unknown)" when the raw word is known.
  test("an unknown stop reason is journaled with its word", () => {
    const h = rig();
    const c = connectEngineStream(h.options);
    h.socket().receive(hello());
    h.socket().receive(
      frame(0, 1, [], { run: run(1, { state: "stopped", stop_reason: "meteor" }) }),
    );
    h.clock.paint();
    const rec = kinds(c).find((x) => x.kind === "stopped");
    expect(rec?.message).toBe("Run stopped (meteor)");
  });
});
