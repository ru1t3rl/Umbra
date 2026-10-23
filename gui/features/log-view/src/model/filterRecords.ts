import type { JournalLevel, JournalRecord, JournalSourceName } from "@umbra/engine-stream";
import { formatRecord } from "./formatRecord.ts";

export interface LogFilter {
  /** The minimum severity: "info" shows everything. */
  readonly level: JournalLevel;
  /** Empty means every source. */
  readonly sources: readonly JournalSourceName[];
  readonly text: string;
}

export const NO_FILTER: LogFilter = { level: "info", sources: [], text: "" };

/** The text a search looks in: the line, the kind, the source id and its label, lowercased. */
const haystacks = new WeakMap<
  (source: JournalSourceName) => string,
  WeakMap<JournalRecord, string>
>();
const ID = (source: JournalSourceName): string => source;

function haystack(r: JournalRecord, label: (source: JournalSourceName) => string): string {
  let cache = haystacks.get(label);
  if (cache === undefined) {
    cache = new WeakMap();
    haystacks.set(label, cache);
  }
  let text = cache.get(r);
  if (text === undefined) {
    text = [formatRecord(r), r.kind, r.source, label(r.source)].join("\n").toLowerCase();
    cache.set(r, text);
  }
  return text;
}

const RANK: Readonly<Record<JournalLevel, number>> = { info: 0, warning: 1, error: 2 };

/**
 * Level, sources and text, AND'd, in input order. The text is trimmed and case-insensitive and matches the
 * formatted line, the kind, the source id and its display label (`sourceLabel`, ASM-944 c, ASM-946). The searchable
 * text is cached per record and label function (records are immutable), so a refilter per journal flush does not
 * reformat 50,000 records. Nothing to filter returns the same array.
 */
export function filterRecords(
  records: readonly JournalRecord[],
  filter: LogFilter,
  sourceLabel: (source: JournalSourceName) => string = ID,
): readonly JournalRecord[] {
  const needle = filter.text.trim().toLowerCase();
  if (filter.level === "info" && filter.sources.length === 0 && needle === "") return records;
  const min = RANK[filter.level];
  return records.filter((r) => {
    if (RANK[r.level] < min) return false;
    if (filter.sources.length > 0 && !filter.sources.includes(r.source)) return false;
    if (needle === "") return true;
    return haystack(r, sourceLabel).includes(needle);
  });
}
