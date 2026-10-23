import type { Affiliation } from "@umbra/ui-kit";
import { MAX_STRING } from "./settings.ts";

export const AFFILIATIONS: readonly Affiliation[] = ["friend", "hostile", "neutral", "unknown"];
export const isAffiliation = (value: string): value is Affiliation =>
  (AFFILIATIONS as readonly string[]).includes(value);

/** Names `parseSettings` drops silently, so the editor says so instead. */
const RESERVED: readonly string[] = ["__proto__", "constructor", "prototype"];

export type RowError = "empty" | "duplicate" | "reserved" | "tooLong";

/** One row of the editor. `key` is the committed, normalized force; `null` when the row contributes nothing. */
export interface OverrideRow {
  id: number;
  /** What the Force field shows. */
  text: string;
  key: string | null;
  affiliation: Affiliation;
  error: RowError | null;
}

export const normalizeForce = (text: string): string => text.trim().toLowerCase();

/** The rows a stored map stands for, in its order. */
export function rowsFromMap(map: Readonly<Record<string, Affiliation>>): OverrideRow[] {
  return Object.entries(map).map(([key, affiliation], id) => ({
    id,
    text: key,
    key,
    affiliation,
    error: null,
  }));
}

/** The stored map is exactly the valid rows (ASM-995 f). */
export function mapFromRows(rows: readonly OverrideRow[]): Record<string, Affiliation> {
  const map: Record<string, Affiliation> = {};
  for (const row of rows)
    if (row.key !== null && row.error === null) map[row.key] = row.affiliation;
  return map;
}

/** Judges the Force text of row `id` against the other rows. A flagged row keeps its text and loses its key. */
export function commitForce(rows: readonly OverrideRow[], id: number, text: string): OverrideRow[] {
  const key = normalizeForce(text);
  const error = ((): RowError | null => {
    if (key === "") return "empty";
    if (RESERVED.includes(key)) return "reserved";
    // `parseSettings` drops keys longer than this silently, so the editor must say so.
    if (key.length > MAX_STRING) return "tooLong";
    if (rows.some((r) => r.id !== id && r.key === key && r.error === null)) return "duplicate";
    return null;
  })();
  return rows.map((r) =>
    r.id !== id
      ? r
      : error === null
        ? { ...r, text: key, key, error }
        : { ...r, text, key: null, error },
  );
}

/**
 * Judges every row flagged `duplicate` again, in order: its twin may be gone (removed or renamed), and a row that
 * was flagged against a twin is never committed again by itself. Of several duplicates of one name the first
 * becomes valid and the later ones stay flagged. Other rows are left as they are.
 */
export function rejudgeDuplicates(rows: readonly OverrideRow[]): OverrideRow[] {
  let next = [...rows];
  for (const row of rows) {
    if (row.error === "duplicate") next = commitForce(next, row.id, row.text);
  }
  return next;
}
