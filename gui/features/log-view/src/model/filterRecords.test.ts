// WI-11 (plan "Model: filterRecords(level, sources, text)"): written before model/filterRecords.ts exists.
// `filterRecords(records, filter)` with `LogFilter = { level, sources, text }` and `NO_FILTER`
// (ASM-944 c): `level` is the minimum severity (info < warning < error; "info" shows everything), `sources`
// empty means every source, otherwise only those; `text` is trimmed, case-insensitive, and matches the
// formatted text (formatRecord), the kind and the source name. Nothing to filter returns the same array.
// Catches: level treated as an exact match, an empty source list hiding everything, case-sensitive search,
// search on the raw message of an event record instead of its formatted text, a copy of 50,000 records.
import { describe, expect, test } from "bun:test";
import { eventRec, rec } from "../logTesting.tsx";
import { filterRecords, type LogFilter, NO_FILTER } from "./filterRecords.ts";

const records = [
  rec(1, {
    source: "connection",
    level: "info",
    kind: "connected",
    message: "Connected to the engine",
  }),
  rec(2, { source: "run", level: "warning", kind: "paused", message: "Run paused" }),
  eventRec(3, { kind: "fire", entity_id: 3, target_id: 7, weapon: "Rifle" }),
  rec(4, { source: "run", level: "error", kind: "stopped", message: "Run stopped (plugin_fault)" }),
  rec(5, {
    source: "engine-process",
    level: "info",
    kind: "line",
    message: "listening on 127.0.0.1",
  }),
];
const seqs = (list: readonly { seq: number }[]) => list.map((r) => r.seq);
const filter = (f: Partial<LogFilter>): LogFilter => ({ ...NO_FILTER, ...f });

describe("filterRecords", () => {
  test("NO_FILTER is every level, every source and no text", () => {
    expect(NO_FILTER).toEqual({ level: "info", sources: [], text: "" });
  });
  test("with nothing to filter it returns the same array", () => {
    expect(filterRecords(records, NO_FILTER)).toBe(records);
    expect(filterRecords(records, filter({ text: "   " }))).toBe(records);
  });
  test("the level is a minimum: warning keeps warnings and errors, error keeps errors", () => {
    expect(seqs(filterRecords(records, filter({ level: "warning" })))).toEqual([2, 4]);
    expect(seqs(filterRecords(records, filter({ level: "error" })))).toEqual([4]);
    expect(seqs(filterRecords(records, filter({ level: "info" })))).toEqual([1, 2, 3, 4, 5]);
  });
  test("sources keep only the listed sources; an empty list keeps all", () => {
    expect(seqs(filterRecords(records, filter({ sources: ["run"] })))).toEqual([2, 4]);
    expect(seqs(filterRecords(records, filter({ sources: ["run", "event"] })))).toEqual([2, 3, 4]);
    expect(seqs(filterRecords(records, filter({ sources: [] })))).toEqual([1, 2, 3, 4, 5]);
  });
  test("the text matches case-insensitively as a substring of the message", () => {
    expect(seqs(filterRecords(records, filter({ text: "RUN" })))).toEqual([2, 4]);
    expect(seqs(filterRecords(records, filter({ text: "  listening " })))).toEqual([5]);
  });
  test("the text matches what the user reads for an event (formatted), not the raw kind alone", () => {
    expect(seqs(filterRecords(records, filter({ text: "fired at #7" })))).toEqual([3]);
    expect(seqs(filterRecords(records, filter({ text: "rifle" })))).toEqual([3]);
  });
  test("the text also matches the kind and the source name", () => {
    expect(seqs(filterRecords(records, filter({ text: "plugin_fault" })))).toEqual([4]);
    expect(seqs(filterRecords(records, filter({ text: "engine-process" })))).toEqual([5]);
    expect(seqs(filterRecords(records, filter({ text: "connected" })))).toEqual([1]);
  });
  test("level, sources and text are AND'd", () => {
    expect(
      seqs(filterRecords(records, filter({ level: "warning", sources: ["run"], text: "stopped" }))),
    ).toEqual([4]);
    expect(filterRecords(records, filter({ level: "error", sources: ["connection"] }))).toEqual([]);
  });
  test("keeps the input order and does not mutate it", () => {
    const copy = [...records];
    const out = filterRecords(records, filter({ sources: ["run", "connection"] }));
    expect(seqs(out)).toEqual([1, 2, 4]);
    expect(records).toEqual(copy);
  });
});

describe("filterRecords: label and cache (m4, n1)", () => {
  const label = (source: string): string => (source === "run" ? "Operation" : source);
  test("the text also matches the translated source label", () => {
    expect(seqs(filterRecords(records, filter({ text: "operation" }), label))).toEqual([2, 4]);
    expect(seqs(filterRecords(records, filter({ text: "operation" })))).toEqual([]);
  });
  test("the searchable text of a record is built once per label function, not per call", () => {
    let calls = 0;
    const counting = (source: string): string => {
      calls += 1;
      return source;
    };
    const f = filter({ text: "listening" });
    filterRecords(records, f, counting);
    const first = calls;
    expect(first).toBe(records.length);
    filterRecords(records, filter({ text: "stopped" }), counting);
    expect(calls).toBe(first);
  });
});
