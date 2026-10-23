import type { JournalRecord } from "@umbra/engine-stream";

/** One `JSON.stringify(record)` per line, each ended by a newline; the empty list gives "" (ASM-944 d). */
export function toJsonLines(records: readonly JournalRecord[]): string {
  let out = "";
  for (const r of records) out += `${JSON.stringify(r)}\n`;
  return out;
}
