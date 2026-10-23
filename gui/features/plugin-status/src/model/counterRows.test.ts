// WI-13 `counterRows(counters)`: the adapter counters as table rows `{ key, label, value }` (ASM-955).
import { describe, expect, test } from "bun:test";
import type { AdapterCountersDto } from "@umbra/data";
import { ADAPTERS } from "../statusTesting.tsx";
import { counterRows } from "./counterRows.ts";

const first = ADAPTERS[0];
if (!first) throw new Error("fixture");
const counters: AdapterCountersDto = first.counters;

describe("counterRows", () => {
  test("gives one row per contract counter, in the display order", () => {
    expect(counterRows(counters).map((r) => r.key)).toEqual([
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
    ]);
  });

  test("labels are words, not wire names", () => {
    const labels = Object.fromEntries(counterRows(counters).map((r) => [r.key, r.label]));
    expect(labels).toMatchObject({
      published: "Published",
      received: "Received",
      tx_dropped: "Outbound dropped",
      rx_dropped: "Inbound dropped",
      decode_errors: "Decode errors",
      io_errors: "I/O errors",
      echo_dropped: "Echoes dropped",
      control_ignored: "Control ignored",
      unmapped: "Unmapped",
      unsupported: "Unsupported",
      invalid_state: "Invalid state",
      ghosts_capped: "Ghosts capped",
      keys_exhausted: "Keys exhausted",
      world_errors: "World errors",
    });
  });

  test("carries the values", () => {
    const byKey = Object.fromEntries(counterRows(counters).map((r) => [r.key, r.value]));
    expect(byKey).toMatchObject({ published: 120, received: 87, rx_dropped: 4, decode_errors: 2 });
    expect(byKey.io_errors).toBe(0);
  });

  test("a counter this version does not know follows the known ones, labelled by its key", () => {
    const rows = counterRows({ ...counters, future_counter: 7, another: 1 } as AdapterCountersDto);
    expect(rows).toHaveLength(16);
    expect(rows.slice(14)).toEqual([
      { key: "another", label: "another", value: 1 },
      { key: "future_counter", label: "future_counter", value: 7 },
    ]);
  });

  test("a missing counter is left out, not shown as zero", () => {
    const { published: _gone, ...rest } = counters;
    const rows = counterRows(rest as AdapterCountersDto);
    expect(rows.map((r) => r.key)).not.toContain("published");
    expect(rows).toHaveLength(13);
  });
});
