// WI-5 / ADR 0071: the journal ring. Assumed API (ASM-895):
//   createJournal(capacity: number, now?: () => number) => { source: JournalSource; append(draft): void; publish(): boolean }
//   `draft` is a JournalRecord without `seq` and `received_at`; `publish` swaps in the new snapshot, notifies
//   once and returns whether anything changed. Records appended since the last publish are staged.
import { describe, expect, spyOn, test } from "bun:test";
import type { JournalRecord } from "../types.ts";
import { createJournal } from "./journal.ts";

type Draft = Omit<JournalRecord, "seq" | "received_at">;
const draft = (n: number, extra: Partial<Draft> = {}): Draft => ({
  source: "event",
  level: "info",
  kind: `k${n}`,
  message: `m${n}`,
  ...extra,
});
const seqs = (j: ReturnType<typeof createJournal>) => j.source.snapshot().map((r) => r.seq);

describe("journal ring", () => {
  test("seq counts up by one from 1 and received_at comes from the injected clock", () => {
    // Mutation caught: seq from 0, per-batch seq, or Date.now() ignoring `now`.
    let t = 100;
    const j = createJournal(10, () => t++);
    for (let i = 0; i < 3; i++) j.append(draft(i));
    j.publish();
    expect(seqs(j)).toEqual([1, 2, 3]);
    expect(j.source.snapshot().map((r) => r.received_at)).toEqual([100, 101, 102]);
  });
  test("the snapshot only changes on publish and keeps its identity when nothing changed", () => {
    // Mutation caught: publishing on append (breaks once-per-flush) or a fresh array per call (useSyncExternalStore loop).
    const j = createJournal(10);
    const empty = j.source.snapshot();
    j.append(draft(1));
    expect(j.source.snapshot()).toBe(empty);
    expect(j.publish()).toBe(true);
    const one = j.source.snapshot();
    expect(one).toHaveLength(1);
    expect(j.publish()).toBe(false);
    expect(j.source.snapshot()).toBe(one);
  });
  test("the snapshot is frozen and earlier snapshots are not mutated by later appends", () => {
    // Mutation caught: pushing into the published array.
    const j = createJournal(10);
    j.append(draft(1));
    j.publish();
    const first = j.source.snapshot();
    j.append(draft(2));
    j.publish();
    expect(first).toHaveLength(1);
    expect(Object.isFrozen(first)).toBe(true);
  });
  test("the oldest records are evicted past capacity and evicted() counts them", () => {
    // Mutation caught: evicting the newest, an off-by-one in capacity, not counting evictions.
    const j = createJournal(3);
    for (let i = 0; i < 5; i++) j.append(draft(i));
    j.publish();
    expect(seqs(j)).toEqual([3, 4, 5]);
    expect(j.source.evicted()).toBe(2);
    expect(j.source.capacity).toBe(3);
  });
  test("a single batch larger than the capacity keeps the newest capacity records", () => {
    // Mutation caught: trimming only one record per append batch.
    const j = createJournal(3);
    for (let i = 0; i < 10; i++) j.append(draft(i));
    j.publish();
    expect(seqs(j)).toEqual([8, 9, 10]);
    expect(j.source.evicted()).toBe(7);
  });
  test("seq keeps counting after eviction", () => {
    // Mutation caught: seq derived from the array length.
    const j = createJournal(2);
    for (let i = 0; i < 4; i++) j.append(draft(i));
    j.publish();
    j.append(draft(9));
    j.publish();
    expect(seqs(j)).toEqual([4, 5]);
  });
  test("many appends notify subscribers once per publish, an empty publish notifies nobody", () => {
    // Mutation caught: notify per append; notify on a no-op publish.
    const j = createJournal(100);
    let n = 0;
    j.source.subscribe(() => {
      n += 1;
    });
    for (let i = 0; i < 50; i++) j.append(draft(i));
    expect(n).toBe(0);
    j.publish();
    expect(n).toBe(1);
    j.publish();
    expect(n).toBe(1);
  });
  test("an unsubscribed listener is not called and a throwing listener does not stop the others", () => {
    // Mutation caught: unsubscribe is a no-op; one listener's error aborts the loop.
    const err = spyOn(console, "error").mockImplementation(() => {});
    const j = createJournal(10);
    const calls: string[] = [];
    const off = j.source.subscribe(() => calls.push("gone"));
    j.source.subscribe(() => {
      throw new Error("boom");
    });
    j.source.subscribe(() => calls.push("kept"));
    off();
    j.append(draft(1));
    j.publish();
    expect(calls).toEqual(["kept"]);
    err.mockRestore();
  });
  test("the draft is copied in verbatim, including detail and unknown kinds", () => {
    // Mutation caught: dropping optional fields or normalising `kind`.
    const j = createJournal(10);
    j.append(
      draft(1, { kind: "future_kind", detail: { a: [1] }, tick: 7, entity_id: 3, session: 2 }),
    );
    j.publish();
    expect(j.source.snapshot()[0]).toMatchObject({
      kind: "future_kind",
      detail: { a: [1] },
      tick: 7,
      entity_id: 3,
      session: 2,
    });
  });
  test("a capacity that is not a positive integer is rejected", () => {
    // Mutation caught: no validation (a 0 capacity would drop everything silently).
    for (const bad of [0, -1, 1.5, Number.NaN]) {
      expect(() => createJournal(bad)).toThrow(RangeError);
    }
  });
});
