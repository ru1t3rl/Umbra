import { expect, test } from "bun:test";
import {
  commitForce,
  mapFromRows,
  type OverrideRow,
  rejudgeDuplicates,
  rowsFromMap,
} from "./overrides.ts";

const blank = (id: number): OverrideRow => ({
  id,
  text: "",
  key: null,
  affiliation: "unknown",
  error: null,
});

test("rows and map round-trip", () => {
  const map = { blue: "hostile", red: "friend" } as const;
  expect(mapFromRows(rowsFromMap(map))).toEqual(map);
});

test("a committed name is normalized and stored", () => {
  const rows = commitForce([blank(0)], 0, "  Blue ");
  expect(rows[0]).toMatchObject({ text: "blue", key: "blue", error: null });
  expect(mapFromRows(rows)).toEqual({ blue: "unknown" });
});

test("empty, reserved and duplicate names are flagged and add nothing", () => {
  const base = rowsFromMap({ blue: "friend" });
  const rows = [...base, blank(5)];
  expect(commitForce(rows, 5, " ")[1]?.error).toBe("empty");
  expect(commitForce(rows, 5, "Constructor")[1]?.error).toBe("reserved");
  const dup = commitForce(rows, 5, "BLUE");
  expect(dup[1]?.error).toBe("duplicate");
  expect(mapFromRows(dup)).toEqual({ blue: "friend" });
});

test("renaming into a duplicate drops the old key", () => {
  const rows = commitForce(rowsFromMap({ blue: "friend", red: "hostile" }), 1, "blue");
  expect(mapFromRows(rows)).toEqual({ blue: "friend" });
});

test("a name longer than the parser keeps (4,096) is flagged", () => {
  const rows = commitForce([blank(0)], 0, "a".repeat(4097));
  expect(rows[0]?.error).toBe("tooLong");
  expect(mapFromRows(rows)).toEqual({});
  expect(commitForce([blank(0)], 0, "a".repeat(4096))[0]?.error).toBeNull();
});

test("rejudgeDuplicates clears a duplicate whose twin is gone and keeps the later twins flagged", () => {
  const flagged = (id: number, text: string): OverrideRow => ({
    id,
    text,
    key: null,
    affiliation: "unknown",
    error: "duplicate",
  });
  const gone = rejudgeDuplicates([flagged(1, "BLUE"), flagged(2, "blue")]);
  expect(gone[0]).toMatchObject({ key: "blue", text: "blue", error: null });
  expect(gone[1]?.error).toBe("duplicate");
  expect(mapFromRows(gone)).toEqual({ blue: "unknown" });
});

test("rejudgeDuplicates leaves other rows and still-valid twins alone", () => {
  const rows = commitForce([...rowsFromMap({ blue: "friend" }), blank(5)], 5, "blue");
  expect(rejudgeDuplicates(rows)).toEqual(rows);
});
