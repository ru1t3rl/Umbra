import { describe, expect, test } from "bun:test";
import { safeStringify } from "./safeStringify.ts";

// Assumed API: safeStringify(value: unknown): string. Never throws. 2-space JSON,
// "[Circular]" for a cycle, "[Too deep]" beyond depth 32, bigint as its decimal string,
// undefined/function/symbol as their typeof-style text, output cut at 20,000 characters + "… (truncated)".
describe("safeStringify", () => {
  test("pretty-prints plain data with two-space indent", () => {
    // Catches: compact output (the JSON view is meant to be read).
    expect(safeStringify({ a: [1, 2] })).toBe(JSON.stringify({ a: [1, 2] }, null, 2));
  });
  test("a cyclic value renders [Circular] instead of throwing", () => {
    const a: Record<string, unknown> = { name: "a" };
    a.self = a;
    // Catches: a bare JSON.stringify (TypeError).
    expect(safeStringify(a)).toContain("[Circular]");
  });
  test("a shared but acyclic reference is printed twice, not as Circular", () => {
    // Catches: a global 'seen' set instead of an ancestor stack.
    const shared = { x: 1 };
    const out = safeStringify({ p: shared, q: shared });
    expect(out).not.toContain("[Circular]");
    expect(out.match(/"x": 1/g)?.length).toBe(2);
  });
  test("a 10,000-deep value does not overflow the stack", () => {
    // Catches: unbounded recursion (RangeError).
    let v: unknown = 1;
    for (let i = 0; i < 10_000; i++) v = { v };
    expect(safeStringify(v)).toContain("[Too deep]");
  });
  test("bigint, undefined, function and getters that throw never throw", () => {
    // Catches: BigInt TypeError and uncaught getter errors.
    const hostile = {
      big: 10n,
      u: undefined,
      f: () => 1,
      get boom(): number {
        throw new Error("no");
      },
    };
    expect(() => safeStringify(hostile)).not.toThrow();
    expect(safeStringify(hostile)).toContain("10");
    expect(() => safeStringify(undefined)).not.toThrow();
    expect(safeStringify(undefined)).toBe("undefined");
  });
  test("output is truncated with a visible marker", () => {
    // Catches: unbounded output for a huge value (DOM size).
    const out = safeStringify({ big: "x".repeat(100_000) });
    expect(out.length).toBeLessThanOrEqual(20_000 + "… (truncated)".length);
    expect(out.endsWith("… (truncated)")).toBe(true);
  });
});
