// WI-7 / ASM-811. Assumed API (model/clockScale.ts):
//   SCALE_RANGE = { min: 0.001, max: 1_000_000 }                (contract: ClockSetting scaled.scale)
//   SLIDER = { min: 0, max: 300, step: 1 }                      (position = 100 per decade over 0.1x..100x)
//   scaleFromPosition(pos: number): number                      (10^(pos/100 - 1), rounded to 3 significant digits)
//   positionFromScale(scale: number): number                    (rounded, clamped to the slider range)
//   parseScale(text: string): number | null                     (plain decimal, within SCALE_RANGE)
//   formatScale(scale: number): string                          ("×4", "×0.5")
import { expect, test } from "bun:test";
import {
  formatScale,
  parseScale,
  positionFromScale,
  SCALE_RANGE,
  SLIDER,
  scaleFromPosition,
} from "./clockScale.ts";

// Catches: a slider that is linear (position 150 would be 55, not 3.16) or off by a decade.
test("the slider is logarithmic over 0.1x to 100x", () => {
  expect(scaleFromPosition(SLIDER.min)).toBe(0.1);
  expect(scaleFromPosition(100)).toBe(1);
  expect(scaleFromPosition(150)).toBe(3.16);
  expect(scaleFromPosition(200)).toBe(10);
  expect(scaleFromPosition(SLIDER.max)).toBe(100);
  expect(SLIDER).toEqual({ min: 0, max: 300, step: 1 });
});

// Catches: the mapping not being invertible (the thumb would jump after Apply).
test("position and scale round-trip for the decade marks", () => {
  for (const p of [0, 100, 200, 300]) expect(positionFromScale(scaleFromPosition(p))).toBe(p);
  expect(positionFromScale(3.16)).toBe(150);
});

// Catches: a scale outside the slider (say 5000) moving the thumb out of range.
test("a scale beyond the slider range clamps the thumb", () => {
  expect(positionFromScale(5000)).toBe(SLIDER.max);
  expect(positionFromScale(0.001)).toBe(SLIDER.min);
});

// Catches: the numeric field limited to the slider range instead of the contract range.
test("the numeric field accepts the full contract range", () => {
  expect(SCALE_RANGE).toEqual({ min: 0.001, max: 1_000_000 });
  expect(parseScale("0.001")).toBe(0.001);
  expect(parseScale("1000000")).toBe(1_000_000);
  expect(parseScale(" 2.5 ")).toBe(2.5);
  expect(parseScale(".5")).toBe(0.5);
});

// Catches: below or above the contract range, and lenient number syntax.
test.each(["0", "0.0009", "1000000.1", "-1", "", "abc", "1e3", "1,5", "Infinity", "NaN", "0x10"])(
  "rejects %p",
  (text) => {
    expect(parseScale(text)).toBeNull();
  },
);

// Catches: a label that hides the factor or prints float noise.
test("formatScale", () => {
  expect(formatScale(4)).toBe("×4");
  expect(formatScale(0.5)).toBe("×0.5");
  expect(formatScale(3.16)).toBe("×3.16");
});
