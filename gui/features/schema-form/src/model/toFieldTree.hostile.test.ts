import { describe, expect, test } from "bun:test";
import { radarSchema } from "../../test/fixtures/schemas.ts";
import { type FieldNode, toFieldTree } from "./toFieldTree.ts";

/*
 * Hostile input (ADR 0070: "never throws on any input"; ASM-897..ASM-900 define the limits):
 *   MAX_DEPTH 8 object nesting levels below the root (a deeper object is a JSON node, reason "depth"),
 *   MAX_NODES 1,000 field nodes per tree (the rest of the traversal becomes one JSON node, reason "budget"),
 *   enums above 200 values and arrays above 200 shown items are capped (see toFieldTree.test.ts).
 * No wall-clock assertions (deterministic). A sync infinite loop cannot be timed out by bun, so every loop is bounded by a hop, depth or node cap and the tests assert the capped result.
 */

function count(n: FieldNode): number {
  return 1 + (n.kind === "object" ? n.children.reduce((a, c) => a + count(c), 0) : 0);
}
function maxDepth(n: FieldNode): number {
  return n.kind === "object" ? 1 + Math.max(0, ...n.children.map(maxDepth)) : 0;
}
function hasKind(n: FieldNode, kind: string): boolean {
  return n.kind === kind || (n.kind === "object" && n.children.some((c) => hasKind(c, kind)));
}
const tree = (s: unknown, v: unknown) => toFieldTree(s, v).root;

describe("hostile schemas", () => {
  test("object nesting 5,000 levels deep is capped at the depth limit with a JSON leaf", () => {
    // Catches: unbounded recursion (RangeError) or no cap on tree depth.
    let schema: Record<string, unknown> = { type: "number" };
    let value: unknown = 1;
    for (let i = 0; i < 5000; i++) {
      schema = { type: "object", properties: { n: schema } };
      value = { n: value };
    }
    const r = tree(schema, value);
    expect(maxDepth(r)).toBeLessThanOrEqual(9);
    expect(hasKind(r, "json")).toBe(true);
  });

  test("a deep subtree is still visible: the depth-cap JSON node contains the value", () => {
    let schema: Record<string, unknown> = { type: "number" };
    let value: unknown = 4242;
    for (let i = 0; i < 20; i++) {
      schema = { type: "object", properties: { n: schema } };
      value = { n: value };
    }
    const find = (n: FieldNode): FieldNode | undefined =>
      n.kind === "json" ? n : n.kind === "object" ? n.children.map(find).find(Boolean) : undefined;
    const j = find(tree(schema, value)) as Extract<FieldNode, { kind: "json" }>;
    expect(j.reason).toBe("depth");
    expect(j.json).toContain("4242");
  });

  test("a JS-object cycle in the schema (properties.self === schema) terminates", () => {
    // Catches: following object identity cycles (not $ref) forever.
    const s: Record<string, unknown> = { type: "object", properties: {} };
    (s.properties as Record<string, unknown>).self = s;
    const v: Record<string, unknown> = {};
    v.self = v;
    const r = tree(s, v);
    expect(maxDepth(r)).toBeLessThanOrEqual(9);
    expect(JSON.stringify(r).length).toBeGreaterThan(0);
  });

  test("a cyclic value never throws and prints [Circular] in the JSON view", () => {
    // Catches: JSON.stringify on a cyclic value.
    const v: Record<string, unknown> = { range_m: "x" };
    v.range_m = v;
    const r = tree(radarSchema, v);
    expect(JSON.stringify(r)).toContain("[Circular]");
  });

  test("a $ref chain a -> b -> a is a recursive-ref JSON node", () => {
    const s = {
      type: "object",
      properties: { p: { $ref: "#/$defs/a" } },
      $defs: { a: { $ref: "#/$defs/b" }, b: { $ref: "#/$defs/a" } },
    };
    const p = (tree(s, { p: 1 }) as Extract<FieldNode, { kind: "object" }>).children[0];
    expect(p).toMatchObject({ kind: "json", reason: "recursive-ref" });
  });

  test("exponential $ref fan-out (30 levels, two refs each) stays within the node budget", () => {
    // Catches: no node budget: this schema expands to 2^30 nodes.
    const defs: Record<string, unknown> = {};
    for (let i = 0; i < 30; i++) {
      const next = i === 29 ? { type: "number" } : { $ref: `#/$defs/d${i + 1}` };
      defs[`d${i}`] = { type: "object", properties: { l: next, r: next } };
    }
    const s = { $ref: "#/$defs/d0", $defs: defs };
    const r = tree(s, {});
    expect(count(r)).toBeLessThanOrEqual(1001);
  });

  test("10,000 properties are capped by the node budget, with a JSON node for the rest", () => {
    // Catches: unbounded child lists (DOM size).
    const properties: Record<string, unknown> = {};
    const value: Record<string, unknown> = {};
    for (let i = 0; i < 10_000; i++) {
      properties[`p${i}`] = { type: "number" };
      value[`p${i}`] = i;
    }
    const r = tree({ type: "object", properties }, value);
    expect(count(r)).toBeLessThanOrEqual(1001);
    const last = (r as Extract<FieldNode, { kind: "object" }>).children.at(-1);
    expect(last).toMatchObject({ kind: "json", reason: "budget" });
    expect((last as { json: string }).json).toContain("p9999");
  });

  test("10,000 extra keys are capped too and the rest is accounted for", () => {
    // Catches: extras bypassing the budget.
    const value: Record<string, unknown> = {};
    for (let i = 0; i < 10_000; i++) value[`k${i}`] = i;
    const r = tree({ type: "object", properties: {} }, value) as Extract<
      FieldNode,
      { kind: "object" }
    >;
    expect(r.extras.length).toBeLessThanOrEqual(1001);
    // The last entry is a visible summary: { key: "…", json: "9000 more keys not shown" } (ASM-898).
    expect(r.extras.at(-1)?.json).toMatch(/more/);
  });

  test("a very long string value is shown, truncated by the JSON view only where a JSON node is used", () => {
    const f = tree(
      { type: "object", properties: { a: { oneOf: [] } } },
      { a: "x".repeat(1_000_000) },
    ) as Extract<FieldNode, { kind: "object" }>;
    expect((f.children[0] as { json: string }).json.length).toBeLessThanOrEqual(20_020);
  });

  test("getters that throw (schema and value) do not throw out of the model", () => {
    // Catches: an uncaught user-code exception; the whole tree becomes a JSON node 'unsupported'.
    const s = {
      type: "object",
      get properties(): never {
        throw new Error("hostile");
      },
    };
    const v = {
      get a(): never {
        throw new Error("hostile");
      },
    };
    expect(() => tree(s, v)).not.toThrow();
    expect(() => tree(radarSchema, v)).not.toThrow();
    expect(tree(s, {}).kind).toBe("json");
  });

  test("a Proxy that throws on every trap does not escape", () => {
    const boom = new Proxy(
      {},
      {
        get() {
          throw new Error("x");
        },
        has() {
          throw new Error("x");
        },
        ownKeys() {
          throw new Error("x");
        },
        getOwnPropertyDescriptor() {
          throw new Error("x");
        },
      },
    );
    expect(() => tree(boom, boom)).not.toThrow();
    expect(() => tree(radarSchema, boom)).not.toThrow();
  });

  test("odd roots and values: functions, symbols, bigint, Date, Map, NaN, Infinity", () => {
    const odd = [
      undefined,
      null,
      0,
      NaN,
      Infinity,
      10n,
      Symbol("s"),
      () => 1,
      new Date(0),
      new Map([[1, 2]]),
      new Set([1]),
      [],
      {},
      "",
      Object.create(null),
    ];
    for (const s of odd) for (const v of odd) expect(() => toFieldTree(s, v)).not.toThrow();
    for (const v of odd) expect(() => toFieldTree(radarSchema, v)).not.toThrow();
  });

  test("a result is always a tree whose every json node is a string and which survives JSON.stringify", () => {
    // Catches: leaking bigint/cyclic references into the tree (the component would crash when rendering).
    const r = toFieldTree({ type: "object", properties: { a: { oneOf: [] } } }, { a: 10n });
    expect(() => JSON.stringify(r)).not.toThrow();
  });

  test("the schema and value arguments are never mutated", () => {
    // Catches: resolving $refs or filling defaults by writing into the input.
    const s = structuredClone(radarSchema);
    const v = { range_m: 1, extra: { a: [1] } };
    const sb = JSON.stringify(s);
    const vb = JSON.stringify(v);
    toFieldTree(s, v);
    expect(JSON.stringify(s)).toBe(sb);
    expect(JSON.stringify(v)).toBe(vb);
  });

  test("the same input gives an equal tree (pure)", () => {
    expect(toFieldTree(radarSchema, { range_m: 3 })).toEqual(
      toFieldTree(radarSchema, { range_m: 3 }),
    );
  });
});

// Deterministic generator (mulberry32); no fast-check dependency (ASM-899).
function rng(seed: number) {
  let a = seed;
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

describe("property test over generated schemas and values (plan WI-6)", () => {
  const TYPES = [
    "object",
    "number",
    "integer",
    "string",
    "boolean",
    "array",
    "null",
    "wibble",
    ["string", "null"],
  ];
  const KEYWORDS = [
    "oneOf",
    "anyOf",
    "allOf",
    "not",
    "if",
    "then",
    "const",
    "enum",
    "default",
    "minimum",
    "exclusiveMinimum",
    "maximum",
    "required",
    "items",
    "prefixItems",
    "patternProperties",
    "additionalProperties",
    "$ref",
    "title",
    "description",
  ];
  const REFS = ["#/$defs/a", "#/$defs/b", "#", "#/definitions/x", "nope", 5, null];

  function genSchema(r: () => number, depth: number): unknown {
    const pick = <T>(xs: T[]): T => xs[Math.floor(r() * xs.length)] as T;
    if (depth > 5 || r() < 0.15)
      return pick([true, false, null, 1, "s", [], {}, { type: pick(TYPES) }]);
    const s: Record<string, unknown> = {};
    if (r() < 0.8) s.type = pick(TYPES);
    if (r() < 0.6) {
      const props: Record<string, unknown> = {};
      for (let i = 0; i < Math.floor(r() * 4); i++)
        props[pick(["a", "b", "range_m", "__proto__", "constructor"])] = genSchema(r, depth + 1);
      s.properties = props;
    }
    for (const k of KEYWORDS) {
      if (r() < 0.15) {
        s[k] =
          k === "$ref"
            ? pick(REFS)
            : k === "required"
              ? pick([["a"], "a", [1], null])
              : k === "enum"
                ? pick([["x", "y"], [1, 2], [], "x"])
                : k === "items"
                  ? genSchema(r, depth + 1)
                  : k === "oneOf" || k === "anyOf" || k === "allOf"
                    ? [genSchema(r, depth + 1)]
                    : pick([1, "x", null, true, [], { a: 1 }, -3, 0.5]);
      }
    }
    if (depth === 0) s.$defs = { a: genSchema(r, 3), b: { $ref: "#/$defs/a" }, x: genSchema(r, 3) };
    return s;
  }
  function genValue(r: () => number, depth: number): unknown {
    const pick = <T>(xs: T[]): T => xs[Math.floor(r() * xs.length)] as T;
    if (depth > 4 || r() < 0.3)
      return pick([undefined, null, 0, -1, 2.5, "x", "", true, false, [], {}, NaN, 10n]);
    if (r() < 0.5) return Array.from({ length: Math.floor(r() * 4) }, () => genValue(r, depth + 1));
    const o: Record<string, unknown> = {};
    for (let i = 0; i < Math.floor(r() * 4); i++)
      o[pick(["a", "b", "range_m", "zzz", "constructor"])] = genValue(r, depth + 1);
    return o;
  }

  test("2,000 seeded random schema/value pairs never throw and always return a bounded tree", () => {
    // Catches: any throw or unbounded output for malformed input (ADR 0070 'never throws').
    const r = rng(0x5eed);
    for (let i = 0; i < 2000; i++) {
      const s = genSchema(r, 0);
      const v = genValue(r, 0);
      let result: ReturnType<typeof toFieldTree> | undefined;
      expect(() => {
        result = toFieldTree(s, v);
      }).not.toThrow();
      expect(result?.root).toBeDefined();
      expect(count(result?.root as FieldNode)).toBeLessThanOrEqual(1001);
    }
  });
});
