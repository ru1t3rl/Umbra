// Pure model: hostile or partial diagnostics never throw and never invent data (ASM-813, ASM-931).
import { describe, expect, test } from "bun:test";
import { formatLocation, MAX_DIAGNOSTICS, toDiagnosticViews } from "./diagnostics.ts";

describe("formatLocation", () => {
  test.each([
    [["a.toml", 12, 5], "a.toml:12:5"],
    [["a.toml", 12, undefined], "a.toml:12"],
    [["a.toml", 12, null], "a.toml:12"],
    [["a.toml", undefined, undefined], "a.toml"],
    // Catches: a column printed without its line.
    [["a.toml", undefined, 5], "a.toml"],
    [["a.toml", null, 5], "a.toml"],
    // Catches: line 0, negative or fractional numbers printed (lines are 1-based integers).
    [["a.toml", 0, 5], "a.toml"],
    [["a.toml", -3, 5], "a.toml"],
    [["a.toml", 1.5, 5], "a.toml"],
    [["a.toml", 4, 0], "a.toml:4"],
    [["a.toml", 4, -1], "a.toml:4"],
    [["a.toml", 4, 2.5], "a.toml:4"],
    [["a.toml", "4", "5"], "a.toml"],
    [["a.toml", Number.NaN, 5], "a.toml"],
    // No file means no location, even with a line.
    [[undefined, 3, 4], null],
    [[null, 3, 4], null],
    [["", 3, 4], null],
    [[7, 3, 4], null],
  ] as const)("%j gives %j", ([file, line, column], expected) => {
    expect(formatLocation(file, line, column)).toBe(expected);
  });
});

describe("toDiagnosticViews", () => {
  const good = { code: "c", message: "m" };

  test.each([null, undefined, 3, "x", {}, { length: 1 }, true])(
    "%j is no list: no views",
    (raw) => {
      expect(toDiagnosticViews(raw)).toEqual([]);
    },
  );

  // Catches: junk entries crashing or shown as empty rows.
  test("entries that are not diagnostics are dropped, good ones keep the server order", () => {
    const views = toDiagnosticViews([
      null,
      3,
      "x",
      [],
      good,
      { code: 1, message: "m" },
      { code: "c" },
      { message: "m" },
      { code: "c", message: 2 },
      { code: "d", message: "n" },
    ]);
    expect(views.map((v) => v.code)).toEqual(["c", "d"]);
  });

  test("severity: warning stays, anything else reads error", () => {
    const sev = (severity: unknown) => toDiagnosticViews([{ ...good, severity }])[0]?.severity;
    expect(sev("warning")).toBe("warning");
    expect(sev("error")).toBe("error");
    expect(sev(undefined)).toBe("error");
    expect(sev("fatal")).toBe("error");
    expect(sev(1)).toBe("error");
  });

  test("location comes from file, line and column", () => {
    const [v] = toDiagnosticViews([{ ...good, file: "f.toml", line: 2, column: 9 }]);
    expect(v?.location).toBe("f.toml:2:9");
  });

  // Catches: notes that are not a list, or hold junk, crashing the list.
  test("notes: a non-array is no notes, junk entries are dropped, a non-string message too", () => {
    expect(toDiagnosticViews([{ ...good, notes: "x" }])[0]?.notes).toEqual([]);
    expect(toDiagnosticViews([{ ...good, notes: { message: "m" } }])[0]?.notes).toEqual([]);
    const notes = toDiagnosticViews([
      {
        ...good,
        notes: [
          null,
          5,
          { message: 5 },
          { message: undefined },
          { message: "see" },
          { message: "there", file: "g.toml", line: 1, column: 1 },
        ],
      },
    ])[0]?.notes;
    expect(notes).toEqual([
      { message: "see", location: null },
      { message: "there", location: "g.toml:1:1" },
    ]);
  });

  test("the list cap constant is positive (the list component enforces it)", () => {
    expect(MAX_DIAGNOSTICS).toBeGreaterThan(0);
  });
});
