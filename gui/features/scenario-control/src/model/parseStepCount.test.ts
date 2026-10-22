// WI-7. Assumed API (model/parseStepCount.ts):
//   STEP_MIN = 1, STEP_MAX = 1_000_000, STEP_PRESETS = [1, 10, 100, 1000] (ASM-810)
//   parseStepCount(text: string): number | null   (decimal digits only, trimmed, integer in 1..1,000,000)
import { expect, test } from "bun:test";
import { parseStepCount, STEP_MAX, STEP_MIN, STEP_PRESETS } from "./parseStepCount.ts";

// Catches: bounds off by one (StepRequest.count is 1..=1000000).
test("accepts the contract range inclusive", () => {
  expect(STEP_MIN).toBe(1);
  expect(STEP_MAX).toBe(1_000_000);
  expect(parseStepCount("1")).toBe(1);
  expect(parseStepCount("1000000")).toBe(1_000_000);
  expect(parseStepCount("  250 ")).toBe(250);
});

// Catches: 0, negatives and over-range reaching the server (422).
test.each(["0", "-1", "1000001", "99999999999999999999"])("rejects %p", (text) => {
  expect(parseStepCount(text)).toBeNull();
});

// Catches: Number()-style leniency (1e3, 0x10, 1.5, "", "  ", "10 ticks", "+5").
test.each(["", "  ", "1e3", "0x10", "1.5", "10 ticks", "+5", "٣", "NaN", "Infinity"])(
  "rejects the non-integer text %p",
  (text) => {
    expect(parseStepCount(text)).toBeNull();
  },
);

// Catches: a preset the numeric field would refuse.
test("presets are 1, 10, 100, 1000 and valid", () => {
  expect(STEP_PRESETS).toEqual([1, 10, 100, 1000]);
  for (const p of STEP_PRESETS) expect(parseStepCount(String(p))).toBe(p);
});
