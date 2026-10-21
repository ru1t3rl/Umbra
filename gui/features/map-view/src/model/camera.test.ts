import { describe, expect, test } from "bun:test";
import {
  bannerPosition,
  FIT_MAX_ZOOM,
  fitOptionsFrom,
  needsFit,
  normalizeInsets,
  withInsets,
} from "./camera.ts";
import { entity, frameOf } from "./testing.ts";

const source = (value: string) => ({ getPropertyValue: () => value });

describe("fitOptionsFrom", () => {
  test("padding is the space-16 token in px, max zoom 15", () => {
    expect(fitOptionsFrom(source(" 64px "))).toEqual({
      padding: { top: 64, right: 64, bottom: 64, left: 64 },
      maxZoom: 15,
    });
    expect(FIT_MAX_ZOOM).toBe(15);
  });
  test("a missing or non-px token throws", () => {
    for (const bad of ["", "4rem", "auto", "-3px"])
      expect(() => fitOptionsFrom(source(bad))).toThrow("--space-16");
  });
});

describe("insets", () => {
  const fit = fitOptionsFrom(source("64px"));
  test("fit padding is space-16 plus the inset on each side", () => {
    expect(withInsets(fit, { top: 80, left: 10 }).padding).toEqual({
      top: 144,
      right: 64,
      bottom: 64,
      left: 74,
    });
    expect(withInsets(fit, undefined).padding).toEqual(fit.padding);
    expect(withInsets(fit, { top: 80 }).maxZoom).toBe(15);
  });
  test("a bad side counts as 0", () => {
    expect(
      normalizeInsets({ top: -5, right: Number.NaN, bottom: Number.POSITIVE_INFINITY, left: 3 }),
    ).toEqual({
      top: 0,
      right: 0,
      bottom: 0,
      left: 3,
    });
  });
  test("the banner sits below the top inset, centred in the free width", () => {
    expect(bannerPosition({ top: 80 })).toEqual({
      top: "calc(80px + var(--space-4))",
      left: "calc(50% + 0px)",
    });
    expect(bannerPosition({ left: 100, right: 20 }).left).toBe("calc(50% + 40px)");
    expect(bannerPosition(undefined).top).toBe("calc(0px + var(--space-4))");
  });
});

describe("needsFit (ASM-307)", () => {
  const withSession = (session: number, n: number) => ({
    ...frameOf(Array.from({ length: n }, (_, i) => entity(i))),
    session,
  });
  test("the first non-empty frame of a session fits; the rest of it does not", () => {
    expect(needsFit(undefined, withSession(1, 2))).toBe(true);
    expect(needsFit(1, withSession(1, 5))).toBe(false);
    expect(needsFit(1, withSession(2, 1))).toBe(true);
  });
  test("an empty frame never fits", () => {
    expect(needsFit(undefined, withSession(1, 0))).toBe(false);
    expect(needsFit(1, withSession(2, 0))).toBe(false);
  });
});
