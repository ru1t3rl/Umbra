// m1: the status count updates at once on the first render and on a filter change, and at most once per
// second (trailing) when only the records changed (ASM-945).
import { afterEach, beforeEach, expect, jest, test } from "bun:test";
import { act, renderHook } from "@testing-library/react";
import { type LogFilter, NO_FILTER } from "../model/filterRecords.ts";
import { THROTTLE_MS, useThrottledCount } from "./useThrottledCount.ts";

beforeEach(() => jest.useFakeTimers());
afterEach(() => jest.useRealTimers());

type Props = { shown: number; total: number; filter: LogFilter };
const run = (initial: Props) =>
  renderHook((p: Props) => useThrottledCount(p.shown, p.total, p.filter), {
    initialProps: initial,
  });
const at = (shown: number, total: number, filter: LogFilter = NO_FILTER): Props => ({
  shown,
  total,
  filter,
});

test("the first render shows the count at once", () => {
  const { result } = run(at(3, 5));
  expect(result.current).toEqual({ shown: 3, total: 5 });
});

test("records changing alone are shown after the interval, not before; bursts collapse to the newest", () => {
  const { result, rerender } = run(at(1, 1));
  rerender(at(2, 2));
  rerender(at(3, 3));
  expect(result.current).toEqual({ shown: 1, total: 1 });
  act(() => jest.advanceTimersByTime(THROTTLE_MS - 1));
  expect(result.current).toEqual({ shown: 1, total: 1 });
  act(() => jest.advanceTimersByTime(1));
  expect(result.current).toEqual({ shown: 3, total: 3 });
});

test("a filter change shows its count at once", () => {
  const { result, rerender } = run(at(5, 5));
  rerender(at(2, 5, { ...NO_FILTER, text: "x" }));
  expect(result.current).toEqual({ shown: 2, total: 5 });
});

test("a filter change cancels a pending update and takes the newest values", () => {
  const { result, rerender } = run(at(1, 1));
  rerender(at(2, 2));
  rerender(at(1, 3, { ...NO_FILTER, text: "x" }));
  expect(result.current).toEqual({ shown: 1, total: 3 });
  act(() => jest.advanceTimersByTime(THROTTLE_MS * 2));
  expect(result.current).toEqual({ shown: 1, total: 3 });
});

test("nothing changing starts no update", () => {
  const { result, rerender } = run(at(1, 1));
  const first = result.current;
  rerender(at(1, 1));
  act(() => jest.advanceTimersByTime(THROTTLE_MS * 2));
  expect(result.current).toBe(first);
});
