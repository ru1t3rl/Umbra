import { describe, expect, test } from "bun:test";
import { FIT_MAX_ZOOM, fitOptionsFrom, needsFit } from "./camera.ts";
import { entity, frameOf } from "./testing.ts";

const source = (value: string) => ({ getPropertyValue: () => value });

describe("fitOptionsFrom", () => {
  test("padding is the space-16 token in px, max zoom 15", () => {
    expect(fitOptionsFrom(source(" 64px "))).toEqual({ padding: 64, maxZoom: 15 });
    expect(FIT_MAX_ZOOM).toBe(15);
  });
  test("a missing or non-px token throws", () => {
    for (const bad of ["", "4rem", "auto", "-3px"])
      expect(() => fitOptionsFrom(source(bad))).toThrow("--space-16");
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
