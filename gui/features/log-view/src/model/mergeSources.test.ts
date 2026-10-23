// WI-11 (plan "Model: mergeSources (by arrival)"): written before model/mergeSources.ts exists.
// `mergeSources(lists: readonly (readonly JournalRecord[])[]): readonly JournalRecord[]`. Each source numbers
// its own records (`seq` restarts per source), so the plan's "by arrival seq" is read as: by `received_at`,
// ties by the order of the lists, then by `seq` (ASM-944 b). Catches: a sort by seq alone (interleaves two
// sources wrongly), an unstable tie, a copy when there is one list, a mutated input.
import { describe, expect, test } from "bun:test";
import { rec } from "../logTesting.tsx";
import { mergeSources } from "./mergeSources.ts";

const at = (seq: number, received_at: number, source: "run" | "engine-process" = "run") =>
  rec(seq, { received_at, source });
const order = (list: readonly { seq: number; source: string }[]) =>
  list.map((r) => `${r.source[0]}${r.seq}`);

describe("mergeSources", () => {
  test("one list comes back as the same array (no copy of up to 50,000 records)", () => {
    const only = [at(1, 10), at(2, 20)];
    expect(mergeSources([only])).toBe(only);
  });
  test("no lists, or only empty lists, give an empty list", () => {
    expect(mergeSources([])).toEqual([]);
    expect(mergeSources([[], []])).toEqual([]);
  });
  test("an empty list next to a full one returns the full one unchanged", () => {
    const full = [at(1, 10)];
    expect(mergeSources([full, []])).toBe(full);
    expect(mergeSources([[], full])).toBe(full);
  });
  test("two sources are merged by arrival time, not by their own seq", () => {
    const journal = [at(1, 100), at(2, 300), at(3, 500)];
    const engine = [at(1, 200, "engine-process"), at(2, 400, "engine-process")];
    expect(order(mergeSources([journal, engine]))).toEqual(["r1", "e1", "r2", "e2", "r3"]);
  });
  test("equal arrival times: the earlier list first, and each list keeps its own order", () => {
    const journal = [at(1, 100), at(2, 100)];
    const engine = [at(1, 100, "engine-process"), at(2, 100, "engine-process")];
    expect(order(mergeSources([journal, engine]))).toEqual(["r1", "r2", "e1", "e2"]);
    expect(order(mergeSources([engine, journal]))).toEqual(["e1", "e2", "r1", "r2"]);
  });
  test("a list that is already in order is merged in a single pass over 50,000 records", () => {
    const a = Array.from({ length: 25_000 }, (_, i) => at(i + 1, i * 2));
    const b = Array.from({ length: 25_000 }, (_, i) => at(i + 1, i * 2 + 1, "engine-process"));
    const started = performance.now();
    const merged = mergeSources([a, b]);
    expect(performance.now() - started).toBeLessThan(500);
    expect(merged).toHaveLength(50_000);
    expect(merged[0]?.source).toBe("run");
    expect(merged[1]?.source).toBe("engine-process");
    expect(merged[49_999]?.received_at).toBe(49_999);
  });
  test("does not mutate its input", () => {
    const journal = [at(1, 100), at(2, 300)];
    const engine = [at(1, 200, "engine-process")];
    const copies = [[...journal], [...engine]];
    mergeSources([journal, engine]);
    expect(journal).toEqual(copies[0] as typeof journal);
    expect(engine).toEqual(copies[1] as typeof engine);
  });
});
