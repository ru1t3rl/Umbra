import { describe, expect, test } from "bun:test";
import { affiliationOf } from "./affiliation.ts";

describe("affiliationOf (ASM-305)", () => {
  test.each([
    ["blue", "friend"],
    ["friend", "friend"],
    ["friendly", "friend"],
    ["red", "hostile"],
    ["hostile", "hostile"],
    ["opfor", "hostile"],
    ["green", "neutral"],
    ["neutral", "neutral"],
    ["white", "neutral"],
  ] as const)("%s -> %s", (force, expected) => {
    expect(affiliationOf(force)).toBe(expected);
  });
  test("case-insensitive and trimmed", () => {
    expect(affiliationOf("BLUE")).toBe("friend");
    expect(affiliationOf("Opfor")).toBe("hostile");
    expect(affiliationOf(" White ")).toBe("neutral");
  });
  test("null, undefined, empty and unrecognised text are unknown", () => {
    for (const f of [null, undefined, "", "purple", "blue2"])
      expect(affiliationOf(f)).toBe("unknown");
  });
  test("prototype keys do not resolve", () => {
    for (const f of ["constructor", "__proto__", "toString", "hasOwnProperty"]) {
      expect(affiliationOf(f)).toBe("unknown");
    }
  });
});
