// WI-14 review round 2 (ASM-988): the bar's measurement-based compact steps and their hysteresis.
import { describe, expect, test } from "bun:test";
import {
  BAR_FIT_LEVELS,
  contentShrank,
  FIT_HYSTERESIS_PX,
  type FitState,
  INITIAL_FIT,
  initialFit,
  nextFit,
} from "./fitLevel.ts";

const at = (state: FitState, client: number, overflow: number) =>
  nextFit(state, { client, overflow });

describe("nextFit", () => {
  test("a bar that fits stays at level 0", () => {
    expect(at(INITIAL_FIT, 1500, 0)).toEqual(INITIAL_FIT);
  });

  test("an unmeasured bar (width 0, no layout) changes nothing", () => {
    expect(at(INITIAL_FIT, 0, 500)).toEqual(INITIAL_FIT);
  });

  test("an overflow steps up one level and remembers the width the level needed", () => {
    const one = at(INITIAL_FIT, 1000, 400); // content wants 1400
    expect(one.level).toBe(1);
    expect(one.need[0]).toBe(1400);
    const two = at(one, 1000, 250);
    expect(two.level).toBe(2);
    expect(two.need[1]).toBe(1250);
  });

  // Catches: a 1 px overflow left in place (the e2e matrix asserts scrollWidth <= clientWidth).
  test("even a 1 px overflow steps up, and no overflow does not", () => {
    expect(at(INITIAL_FIT, 1000, 1).level).toBe(1);
    expect(at(INITIAL_FIT, 1000, 0).level).toBe(0);
  });

  test("level 2 is the last step: a still-overflowing bar stays there", () => {
    const two = at(at(INITIAL_FIT, 1000, 400), 1000, 250);
    expect(at(two, 1000, 100).level).toBe(2);
  });

  test("it steps back down only when the bar is wider than the level below needed, plus the hysteresis", () => {
    const two = at(at(INITIAL_FIT, 1000, 400), 1000, 250); // need = [1400, 1250, 0]
    expect(at(two, 1250 + FIT_HYSTERESIS_PX - 1, 0).level).toBe(2);
    expect(at(two, 1250 + FIT_HYSTERESIS_PX, 0).level).toBe(1);
  });

  // Catches: flip-flop at the boundary (the content that just overflowed fits again by one pixel).
  test("a width that just overflowed does not step back down", () => {
    const one = at(INITIAL_FIT, 1399, 1 + 1); // overflow 2 px: need 1401
    expect(one.level).toBe(1);
    expect(at(one, 1401, 0).level).toBe(1);
    expect(at(one, 1401 + FIT_HYSTERESIS_PX, 0).level).toBe(0);
  });

  test("a level never visited has no remembered need and is not entered by growing", () => {
    expect(at({ level: 2, need: [0, 0, 0] }, 5000, 0).level).toBe(2);
  });

  // Catches: stale needs (more digits make a level wider).
  test("an overflow at a level refreshes the need of that level", () => {
    const one = at(INITIAL_FIT, 1000, 400);
    const refreshed = at(one, 1000, 300);
    expect(refreshed.need[1]).toBe(1300);
  });
});

describe("contentShrank", () => {
  // Catches: a counter reset (an unload) leaving the remembered, inflated needs in force.
  test("only a shrink of more than the hysteresis counts", () => {
    expect(contentShrank(1000, 1000 - FIT_HYSTERESIS_PX)).toBe(false);
    expect(contentShrank(1000, 1000 - FIT_HYSTERESIS_PX - 1)).toBe(true);
    expect(contentShrank(1000, 1200)).toBe(false);
  });
  test("no previous measurement is not a shrink", () => {
    expect(contentShrank(null, 500)).toBe(false);
  });
});

// WI-15 (ASM-992): a longer ladder, the dashboard's four levels (the first step is the icon-only navigation).
describe("a four-level ladder", () => {
  test("it starts at level 0 with four unmeasured levels", () => {
    expect(initialFit(BAR_FIT_LEVELS)).toEqual({ level: 0, need: [0, 0, 0, 0] });
  });

  test("an overflow climbs through all four levels and stops at the last", () => {
    let state = initialFit(4);
    for (const level of [1, 2, 3, 3] as const) {
      state = at(state, 1000, 100);
      expect(state.level).toBe(level);
    }
    expect(state.need).toEqual([1100, 1100, 1100, 1100]);
  });

  test("it steps back down one level at a time with the hysteresis", () => {
    let state = initialFit(4);
    state = at(at(at(state, 1000, 300), 1000, 200), 1000, 100); // need [1300, 1200, 1100, 0], level 3
    expect(at(state, 1100 + FIT_HYSTERESIS_PX, 0).level).toBe(2);
    expect(at(state, 1100 + FIT_HYSTERESIS_PX - 1, 0).level).toBe(3);
  });
});
