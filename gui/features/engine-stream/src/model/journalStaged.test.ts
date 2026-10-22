import { expect, test } from "bun:test";
import { createJournal } from "./journal.ts";

test("staged records stay bounded without a publish and the newest survive", () => {
  // Mutation caught: removing the 2x-capacity trim (unbounded memory between flushes).
  const n = 10;
  const j = createJournal(n);
  for (let i = 0; i < 5 * n; i++)
    j.append({ source: "event", level: "info", kind: `k${i}`, message: "" });
  j.publish();
  const snap = j.source.snapshot();
  expect(snap).toHaveLength(n);
  expect(snap.map((r) => r.seq)).toEqual(Array.from({ length: n }, (_, i) => 4 * n + i + 1));
  expect(j.source.evicted()).toBe(4 * n);
});

test("the bound holds before the publish: nothing visible, staged does not grow past 2x", () => {
  // Mutation caught: as above, observed through the eviction count that the trim adds before any publish.
  const n = 10;
  const j = createJournal(n);
  for (let i = 0; i < 5 * n; i++)
    j.append({ source: "event", level: "info", kind: "k", message: "" });
  expect(j.source.snapshot()).toHaveLength(0);
  expect(j.source.evicted()).toBeGreaterThanOrEqual(5 * n - 2 * n + 1);
});
