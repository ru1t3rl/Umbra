// WI-11 (plan "Model: toJsonLines (download)"): written before model/toJsonLines.ts exists.
// `toJsonLines(records): string`: one `JSON.stringify(record)` per line, each line ended by "\n", the record
// exactly as the journal holds it (all fields, `detail` included). Empty list: empty string (ASM-944 d).
// Catches: a missing trailing newline, an array wrapper, dropped fields, a pretty-printed (multi-line) record.
import { describe, expect, test } from "bun:test";
import { eventRec, rec } from "../logTesting.tsx";
import { toJsonLines } from "./toJsonLines.ts";

describe("toJsonLines", () => {
  test("an empty list is the empty string", () => {
    expect(toJsonLines([])).toBe("");
  });
  test("each record is one line of JSON ended by a newline", () => {
    const records = [rec(1), eventRec(2, { kind: "fire", entity_id: 3, target_id: 7 })];
    const text = toJsonLines(records);
    expect(text.endsWith("\n")).toBe(true);
    const lines = text.split("\n");
    expect(lines).toHaveLength(3);
    expect(lines[2]).toBe("");
    expect(JSON.parse(lines[0] as string)).toEqual(records[0] as object);
    expect(JSON.parse(lines[1] as string)).toEqual(records[1] as object);
  });
  test("every field survives, including detail and unknown kinds", () => {
    const r = rec(7, {
      source: "event",
      level: "warning",
      kind: "weather_changed",
      detail: { nested: { a: [1, 2] } },
      session: 2,
      tick: 40,
    });
    expect(JSON.parse(toJsonLines([r]))).toEqual(r as object);
  });
  test("a message with a newline stays on one line (escaped)", () => {
    const text = toJsonLines([rec(1, { message: "a\nb" })]);
    expect(text.split("\n")).toHaveLength(2);
    expect(JSON.parse(text.split("\n")[0] as string).message).toBe("a\nb");
  });
});
