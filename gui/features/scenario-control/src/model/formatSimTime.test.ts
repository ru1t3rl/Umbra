import { expect, test } from "bun:test";
import { formatSimTime } from "./formatSimTime.ts";

const table: [number | null | undefined, string][] = [
  [0, "T+00:00:00"],
  [0.999, "T+00:00:00"],
  [1, "T+00:00:01"],
  [59.9, "T+00:00:59"],
  [60, "T+00:01:00"],
  [3599, "T+00:59:59"],
  [3600, "T+01:00:00"],
  [86399, "T+23:59:59"],
  [86400, "T+24:00:00"],
  [99 * 3600 + 59 * 60 + 59, "T+99:59:59"],
  [100 * 3600, "T+100:00:00"],
  [1234 * 3600 + 5, "T+1234:00:05"],
  [-1, "—"],
  [-0.5, "—"],
  [Number.NaN, "—"],
  [Number.POSITIVE_INFINITY, "—"],
  [Number.NEGATIVE_INFINITY, "—"],
  [null, "—"],
  [undefined, "—"],
];

test.each(table)("formatSimTime(%p) is %p", (input, expected) => {
  expect(formatSimTime(input)).toBe(expected);
});
