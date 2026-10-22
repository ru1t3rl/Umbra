import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { ROW_HEIGHT_PX } from "./rowHeights.ts";

interface RowToken {
  name: string;
  value: string;
}

test("ROW_HEIGHT_PX equals the row-height tokens in tokens.json", () => {
  const json = JSON.parse(readFileSync(join(import.meta.dir, "../tokens/tokens.json"), "utf8")) as {
    rowHeight: { tokens: RowToken[] };
  };
  const fromJson = Object.fromEntries(
    json.rowHeight.tokens.map((t) => [t.name, Number.parseInt(t.value, 10)]),
  );
  expect(fromJson).toEqual({ ...ROW_HEIGHT_PX });
  for (const t of json.rowHeight.tokens) expect(t.value).toMatch(/^\d+px$/);
});
