import { describe, expect, test } from "bun:test";
import { radarSchema } from "../../test/fixtures/schemas.ts";
import { BUDGET_KEY } from "./fieldNode.ts";
import { safeStringify } from "./safeStringify.ts";
import { type FieldNode, toFieldTree } from "./toFieldTree.ts";

type Obj = Extract<FieldNode, { kind: "object" }>;
type Json = Extract<FieldNode, { kind: "json" }>;
const root = (s: unknown, v: unknown): FieldNode => toFieldTree(s, v).root;
const kids = (s: unknown, v: unknown): FieldNode[] => (root(s, v) as Obj).children;
const one = (props: Record<string, unknown>, v: unknown, key: string): FieldNode =>
  kids({ type: "object", properties: props }, v).find((c) => c.key === key) as FieldNode;

describe("$ref chains", () => {
  test("a linear chain of 10,000 refs is a ref-chain JSON node, quickly", () => {
    // Catches: following every hop (quadratic Set copies / unbounded loop).
    const defs: Record<string, unknown> = {};
    for (let i = 0; i < 10_000; i++) {
      defs[`d${i}`] = i === 9999 ? { type: "number" } : { $ref: `#/$defs/d${i + 1}` };
    }
    const s = { type: "object", properties: { a: { $ref: "#/$defs/d0" } }, $defs: defs };
    expect(kids(s, { a: 1 })[0]).toMatchObject({ kind: "json", reason: "ref-chain" });
  });
  test("a chain of a few refs still resolves", () => {
    const s = {
      type: "object",
      properties: { a: { $ref: "#/$defs/x" } },
      $defs: { x: { $ref: "#/$defs/y" }, y: { type: "number" } },
    };
    expect(kids(s, { a: 1 })[0]?.kind).toBe("number");
  });
});

describe("help for JSON nodes", () => {
  test("a JSON node keeps description, required and the default (the probe)", () => {
    // Catches: JSON nodes losing the help that other fields show.
    const s = {
      type: "object",
      required: ["x"],
      properties: { x: { oneOf: [], description: "Help!", default: 5 } },
    };
    expect(kids(s, {})[0]).toMatchObject({
      kind: "json",
      description: "Help!",
      required: true,
      defaultText: "5",
      fromDefault: true,
    });
  });
  test("a given value on a JSON node is not from default", () => {
    const s = { type: "object", properties: { x: { oneOf: [], default: 5 } } };
    expect(kids(s, { x: 1 })[0]).toMatchObject({ defaultText: "5", fromDefault: false });
  });
});

describe("budget node", () => {
  test("its summary comes first and survives the length cut with large skipped values", () => {
    // Catches: summary appended last (cut off) or uncapped values filling the view.
    const properties: Record<string, unknown> = {};
    const value: Record<string, unknown> = {};
    for (let i = 0; i < 1300; i++) {
      properties[`p${i}`] = { type: "number" };
      value[`p${i}`] = i < 1100 ? i : "y".repeat(50_000);
    }
    const last = (kids({ type: "object", properties }, value).at(-1) ?? {}) as Json;
    expect(last.reason).toBe("budget");
    expect(last.json.indexOf("more fields not shown")).toBeGreaterThan(-1);
    expect(last.json).toContain("p1299");
    expect(last.json.indexOf("more fields")).toBeLessThan(200);
  });
  test("each listed skipped value is capped", () => {
    const properties: Record<string, unknown> = {};
    const value: Record<string, unknown> = {};
    for (let i = 0; i < 1100; i++) {
      properties[`p${i}`] = { type: "number" };
      value[`p${i}`] = "z".repeat(5000);
    }
    const last = kids({ type: "object", properties }, value).at(-1) as Json;
    expect(last.json.length).toBeLessThan(20_100);
    expect(last.json).not.toContain("z".repeat(200));
  });
  test("its key cannot be produced by a property, even one named like the marker", () => {
    // Catches: React key collision with a real "…" property.
    const properties: Record<string, unknown> = { "…": { type: "number" } };
    for (let i = 0; i < 1100; i++) properties[`p${i}`] = { type: "number" };
    const c = kids({ type: "object", properties }, {});
    expect(c.at(-1)?.key).toBe(BUDGET_KEY);
    expect(new Set(c.map((x) => x.key)).size).toBe(c.length);
  });
});

describe("tree JSON character budget", () => {
  test("after 200,000 characters of JSON text later views say the text is not shown", () => {
    // Catches: 1,000 JSON nodes of 20,000 characters each (20 MB of DOM text).
    const properties: Record<string, unknown> = {};
    const value: Record<string, unknown> = {};
    for (let i = 0; i < 100; i++) {
      properties[`p${i}`] = { oneOf: [] };
      value[`p${i}`] = "q".repeat(50_000);
    }
    const nodes = kids({ type: "object", properties }, value) as Json[];
    const total = nodes.reduce((a, n) => a + n.json.length, 0);
    expect(total).toBeLessThan(260_000);
    expect(nodes.at(-1)?.json).toBe("JSON text not shown (size limit)");
    expect(nodes[0]?.json.length).toBeGreaterThan(10_000);
  });
});

describe("value shapes that are not records", () => {
  test.each([
    ["Date", () => new Date(0)],
    ["Map", () => new Map([["a", 1]])],
    [
      "class instance",
      () =>
        new (class Foo {
          a = 1;
        })(),
    ],
  ])("a %s where an object is expected is a type-mismatch", (_n, make) => {
    // Catches: treating any typeof "object" as a record.
    expect(root({ type: "object", properties: {} }, make())).toMatchObject({
      kind: "json",
      reason: "type-mismatch",
    });
    const f = one({ o: { type: "object", properties: {} } }, { o: make() }, "o");
    expect(f).toMatchObject({ kind: "json", reason: "type-mismatch" });
  });
  test("a null-prototype object is a record", () => {
    const v = Object.create(null);
    v.a = 1;
    expect(root({ type: "object", properties: { a: { type: "number" } } }, v).kind).toBe("object");
  });
});

describe("enum and array details", () => {
  test("duplicate enum values are listed once, in order", () => {
    const f = one({ m: { type: "string", enum: ["a", "b", "a", "c"] } }, {}, "m");
    expect(f).toMatchObject({ options: ["a", "b", "c"] });
  });
  test("a default that is not an option is ignored", () => {
    // Catches: selecting a value that is not in the list.
    const f = one({ m: { type: "string", enum: ["a", "b"], default: "zzz" } }, {}, "m") as Extract<
      FieldNode,
      { kind: "enum" }
    >;
    expect(f.selected).toBeUndefined();
    expect(f.fromDefault).toBe(false);
    expect(f).not.toHaveProperty("defaultText");
  });
  test("an array default with wrong items is ignored", () => {
    const f = one({ p: { type: "array", items: { type: "number" }, default: [1, "x"] } }, {}, "p");
    expect(f).toMatchObject({ kind: "array", items: [], fromDefault: false });
    expect(f).not.toHaveProperty("defaultText");
  });
  test("a valid array default is shown and flagged", () => {
    const f = one({ p: { type: "array", items: { type: "number" }, default: [1, 2] } }, {}, "p");
    expect(f).toMatchObject({ items: ["1", "2"], fromDefault: true, defaultText: "1, 2" });
  });
});

describe("hostile access stays local", () => {
  test("a throwing value getter turns only that field into JSON; siblings stay", () => {
    // Catches: one bad property losing the whole object (readSafe around own(record,key)).
    const v = {
      channels: 3,
      get range_m(): number {
        throw new Error("boom");
      },
    };
    const c = (root(radarSchema, v) as Obj).children;
    expect(c.find((x) => x.key === "range_m")?.kind).toBe("json");
    expect(c.find((x) => x.key === "channels")).toMatchObject({ kind: "integer", text: "3" });
    expect(c.length).toBeGreaterThan(5);
  });
  test("a throwing property-schema getter turns only that field into JSON", () => {
    // Catches: reading props[key] outside the per-child guard.
    const properties = {
      ok: { type: "number" },
      get bad(): never {
        throw new Error("boom");
      },
      also: { type: "string" },
    };
    const c = kids({ type: "object", properties }, { ok: 1, bad: 2, also: "x" });
    expect(c.map((x) => x.kind)).toEqual(["number", "json", "string"]);
  });
  test("a value whose ownKeys throws keeps the fields and marks the extras unreadable", () => {
    // Catches: one hostile trap discarding the whole object.
    const target = { channels: 3 };
    const v = new Proxy(target, {
      ownKeys() {
        throw new Error("boom");
      },
    });
    const r = root(radarSchema, v) as Obj;
    expect(r.kind).toBe("object");
    expect(r.children.find((x) => x.key === "channels")).toMatchObject({ text: "3" });
    expect(r.extras).toHaveLength(1);
    expect(r.extras[0]?.summary).toBe(true);
  });
});

describe("safeStringify visit cap", () => {
  test("a value with 20,000 objects is not walked past 10,000 visits", () => {
    // Catches: no MAX_VISITS (unbounded CPU before the character cut).
    let reads = 0;
    const make = () => {
      const o = {};
      Object.defineProperty(o, "x", {
        enumerable: true,
        get() {
          reads++;
          return 1;
        },
      });
      return o;
    };
    const outer = Array.from({ length: 40 }, () => Array.from({ length: 500 }, make));
    safeStringify(outer);
    expect(reads).toBeLessThanOrEqual(10_000);
    expect(reads).toBeGreaterThan(5000);
  });
});

test("an array whose items schema is a ref chain beyond the hop cap says ref-chain", () => {
  // Catches: ref-chain reported as unresolved-ref.
  const defs: Record<string, unknown> = {};
  for (let i = 0; i < 100; i++) defs[`d${i}`] = { $ref: `#/$defs/d${i + 1}` };
  defs.d100 = { type: "number" };
  const s = {
    type: "object",
    properties: { a: { type: "array", items: { $ref: "#/$defs/d0" } } },
    $defs: defs,
  };
  expect(kids(s, { a: [1] })[0]).toMatchObject({ kind: "json", reason: "ref-chain" });
});
