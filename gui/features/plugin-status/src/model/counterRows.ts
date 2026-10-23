import type { AdapterCountersDto } from "@umbra/data";
import { type MessageKey, t } from "../messages.ts";

export interface CounterRow {
  readonly key: string;
  readonly label: string;
  readonly value: number;
}

/** The contract counters in display order (ASM-955 b). */
const KNOWN = [
  "published",
  "received",
  "tx_dropped",
  "rx_dropped",
  "decode_errors",
  "io_errors",
  "echo_dropped",
  "control_ignored",
  "unmapped",
  "unsupported",
  "invalid_state",
  "ghosts_capped",
  "keys_exhausted",
  "world_errors",
] as const satisfies readonly (keyof AdapterCountersDto)[];

/** Counter rows: the known counters in display order (a missing one is left out), then unknown ones by key. */
export function counterRows(counters: AdapterCountersDto): CounterRow[] {
  const values: Record<string, unknown> = { ...counters };
  const known = new Set<string>(KNOWN);
  const rows: CounterRow[] = [];
  for (const key of KNOWN) {
    const value = values[key];
    if (typeof value === "number")
      rows.push({ key, label: t(`counter.${key}` as MessageKey), value });
  }
  for (const key of Object.keys(values).sort()) {
    const value = values[key];
    if (!known.has(key) && typeof value === "number") rows.push({ key, label: key, value });
  }
  return rows;
}
