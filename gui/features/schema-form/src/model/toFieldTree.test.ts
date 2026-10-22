import { describe, expect, test } from "bun:test";
import { builtinIds, builtinSchema, radarSchema, radarValue } from "../../test/fixtures/schemas.ts";
import { type FieldNode, toFieldTree } from "./toFieldTree.ts";

/*
 * Assumed API (ASM-897..ASM-900), all internal to the package (ADR 0070: the model is not public):
 *
 *   toFieldTree(schema: unknown, value: unknown): { root: FieldNode }      never throws
 *   interface FieldBase { key: string; path: string[]; label: string; description?: string;
 *                         required: boolean }
 *   FieldNode =
 *     | Base & { kind: "object"; children: FieldNode[]; extras: { key: string; json: string }[] }
 *     | Base & { kind: "number" | "integer"; text: string; unit?: string; bounds?: string;
 *                defaultText?: string; fromDefault: boolean }
 *     | Base & { kind: "string"; text: string; defaultText?: string; fromDefault: boolean }
 *     | Base & { kind: "boolean"; checked: boolean; defaultText?: string; fromDefault: boolean }
 *     | Base & { kind: "enum"; options: string[]; selected: string | undefined;
 *                control: "chips" | "select"; defaultText?: string; fromDefault: boolean }
 *     | Base & { kind: "array"; items: string[]; more: number; unit?: string; defaultText?: string; fromDefault: boolean }
 *     | Base & { kind: "json"; json: string; note: "Shown as JSON"; reason: JsonReason }
 *   `path` is the property-name chain from the root (the root has []). `key` is the last element ("" for root).
 *   `label` = the property's own `title`, else the resolved `$ref` target's `title`, else `key`.
 *   `description` is the schema `description`. `defaultText` = String(default) (arrays: items joined ", ").
 *   `fromDefault` is true only when the value is absent (undefined) and a default is used for `text`.
 */

const kids = (n: FieldNode): FieldNode[] => (n.kind === "object" ? n.children : []);
function field(n: FieldNode, ...path: string[]): FieldNode {
  let cur = n;
  for (const key of path) {
    const next = kids(cur).find((c) => c.key === key);
    if (!next) throw new Error(`no field ${key} under ${cur.key || "root"}`);
    cur = next;
  }
  return cur;
}
const root = (schema: unknown, value: unknown) => toFieldTree(schema, value).root;

describe("object and primitive fields (ADR 0070 subset)", () => {
  test("a flat engine schema gives one field per property, in schema order, with kind, label and description", () => {
    // Catches: reordering (alphabetical), dropping a property, label not taken from the key.
    const r = root(builtinSchema("builtin.visual"), { range_m: 3000, fov_deg: 120 });
    expect(r.kind).toBe("object");
    expect(kids(r).map((c) => [c.key, c.kind, c.label])).toEqual([
      ["range_m", "number", "range_m"],
      ["fov_deg", "number", "fov_deg"],
      ["eye_height_m", "number", "eye_height_m"],
    ]);
    expect(field(r, "range_m").description).toBe("Maximum 3D detection distance in meters.");
  });

  test("the root label is the schema title", () => {
    // Catches: ignoring `title`.
    expect(root(builtinSchema("builtin.visual"), {}).label).toBe("builtin.visual parameters");
  });

  test("required keys are marked and others are not", () => {
    // Catches: marking every field, none, or reading required from the wrong level (nested objects).
    const r = root(builtinSchema("builtin.visual"), {});
    expect(kids(r).map((c) => c.required)).toEqual([true, true, false]);
    const radar = root(radarSchema, radarValue);
    expect(field(radar, "antenna", "gain_db").required).toBe(true);
    expect(field(radar, "antenna", "beam_width_deg").required).toBe(false);
    expect(field(radar, "range_m").required).toBe(true);
  });

  test("a given number shows as its String() text, not a default", () => {
    // Catches: showing the default when a value exists.
    const f = field(
      root(builtinSchema("builtin.visual"), { range_m: 1200, fov_deg: 360, eye_height_m: 1.5 }),
      "eye_height_m",
    );
    expect(f).toMatchObject({ kind: "number", text: "1.5", fromDefault: false, defaultText: "2" });
  });

  test("an absent value with a schema default shows the default and says so", () => {
    // Catches: empty text for absent, or fromDefault never set (plan: 'defaults shown when the value is absent').
    const f = field(
      root(builtinSchema("builtin.visual"), { range_m: 1, fov_deg: 1 }),
      "eye_height_m",
    );
    expect(f).toMatchObject({ text: "2", fromDefault: true, defaultText: "2" });
  });

  test("an absent value without a default is empty text and not from default", () => {
    const f = field(root(builtinSchema("builtin.visual"), {}), "range_m");
    expect(f).toMatchObject({ text: "", fromDefault: false });
    expect(f).not.toHaveProperty("defaultText");
  });

  test("an explicit zero or false is a given value, not absent", () => {
    // Catches: falsy checks (`value ? ... : default`) instead of `=== undefined`.
    const z = field(root(builtinSchema("builtin.visual"), { eye_height_m: 0 }), "eye_height_m");
    expect(z).toMatchObject({ text: "0", fromDefault: false });
    const b = field(root(radarSchema, { enabled: false }), "enabled");
    expect(b).toMatchObject({ kind: "boolean", checked: false, fromDefault: false });
  });

  test("units come from the key suffix and bounds become help text", () => {
    // Catches: unit missing from the field, bounds ignored; ground-kinematic uses three suffixes.
    const r = root(builtinSchema("builtin.ground.kinematic"), {
      max_speed_mps: 18,
      max_turn_rate_dps: 40,
      max_accel_mps2: 2.5,
    });
    expect(field(r, "max_speed_mps")).toMatchObject({
      unit: "m/s",
      bounds: "greater than 0, max 1000",
    });
    expect(field(r, "max_turn_rate_dps")).toMatchObject({ unit: "°/s" });
    expect(field(r, "max_accel_mps2")).toMatchObject({ unit: "m/s²" });
  });

  test("a field without a unit suffix has no unit key", () => {
    const f = field(
      root(builtinSchema("builtin.cond.chance"), { probability: 0.5 }),
      "probability",
    );
    expect(f).toMatchObject({ bounds: "min 0, max 1", text: "0.5" });
    expect(f).not.toHaveProperty("unit");
  });

  test("integer is its own kind", () => {
    // Catches: collapsing integer into number.
    expect(field(root(radarSchema, { channels: 4 }), "channels")).toMatchObject({
      kind: "integer",
      text: "4",
    });
  });

  test("string and boolean fields", () => {
    const r = root(builtinSchema("builtin.action.fire"), {
      target_key: "target",
      weapon: "example.weapon.main_gun",
    });
    expect(field(r, "weapon")).toMatchObject({
      kind: "string",
      text: "example.weapon.main_gun",
      required: true,
    });
    expect(field(root(radarSchema, {}), "enabled")).toMatchObject({
      kind: "boolean",
      checked: true,
      fromDefault: true,
    });
  });

  test("an object schema with no properties (follow_route) is an empty object with no extras", () => {
    // Catches: crashing or showing JSON for an empty properties object.
    const r = root(builtinSchema("builtin.action.follow_route"), {});
    expect(r).toMatchObject({ kind: "object", children: [], extras: [] });
  });

  test("an unknown keyword that does not change the shape (examples, additionalProperties:false, pattern) is ignored", () => {
    // Catches: falling back to JSON for annotations (every built-in schema has `examples`).
    const r = root(
      {
        type: "object",
        properties: { a: { type: "string", pattern: "^a", format: "x", minLength: 1 } },
        examples: [{}],
      },
      { a: "abc" },
    );
    expect(field(r, "a").kind).toBe("string");
  });

  test("every built-in M6 schema becomes an object tree with no JSON fallback anywhere", () => {
    // Catches: a built-in schema outside the subset (fixture drift, plan risk) and any throw.
    for (const id of builtinIds()) {
      const walk = (n: FieldNode): void => {
        expect(n.kind).not.toBe("json");
        for (const c of kids(n)) walk(c);
      };
      const r = root(builtinSchema(id), {});
      expect(r.kind).toBe("object");
      walk(r);
    }
  });
});

describe("enum", () => {
  test("up to five values use chips, more use a select (ADR 0070)", () => {
    // Catches: threshold off by one (5 vs 6) in either direction.
    const five = {
      type: "object",
      properties: { m: { type: "string", enum: ["a", "b", "c", "d", "e"] } },
    };
    const six = {
      type: "object",
      properties: { m: { type: "string", enum: ["a", "b", "c", "d", "e", "f"] } },
    };
    expect(field(root(five, { m: "b" }), "m")).toMatchObject({
      kind: "enum",
      control: "chips",
      selected: "b",
    });
    expect(field(root(six, { m: "b" }), "m")).toMatchObject({
      kind: "enum",
      control: "select",
      selected: "b",
    });
  });

  test("the default selects when absent, and the options keep schema order", () => {
    const f = field(root(radarSchema, {}), "mode");
    expect(f).toMatchObject({
      options: ["search", "track"],
      selected: "search",
      fromDefault: true,
    });
  });

  test("a value outside the enum is shown as JSON, not hidden or selected", () => {
    // Catches: silently selecting nothing / dropping an invalid value.
    const f = field(root(radarSchema, { mode: "jam" }), "mode");
    expect(f).toMatchObject({ kind: "json", reason: "type-mismatch" });
    expect((f as { json: string }).json).toContain("jam");
  });

  test("a non-string enum falls back to JSON", () => {
    // Catches: stringifying numeric enums (ADR 0070 lists string enums only).
    const f = field(
      root({ type: "object", properties: { n: { type: "integer", enum: [1, 2, 3] } } }, { n: 2 }),
      "n",
    );
    expect(f).toMatchObject({ kind: "json", reason: "unsupported" });
  });

  test("a huge enum (more than 200 values) falls back to JSON instead of building 100,000 options", () => {
    // Catches: no cap on enum size.
    const big = Array.from({ length: 100_000 }, (_, i) => `v${i}`);
    const f = field(
      root({ type: "object", properties: { n: { type: "string", enum: big } } }, { n: "v5" }),
      "n",
    );
    expect(f).toMatchObject({ kind: "json", reason: "enum-too-large" });
    expect((f as { json: string }).json.length).toBeLessThan(25_000);
  });
});

describe("arrays of primitives", () => {
  test("a numeric array lists its items as text", () => {
    const f = field(root(radarSchema, radarValue), "detonate_position");
    expect(f).toMatchObject({ kind: "array", items: ["1", "2.5", "3"], more: 0 });
  });

  test("an array longer than 200 items shows 200 and counts the rest in `more`", () => {
    // Catches: unbounded DOM, or dropping the rest without saying so.
    const f = field(
      root(radarSchema, { detonate_position: Array.from({ length: 1000 }, (_, i) => i) }),
      "detonate_position",
    );
    expect(f).toMatchObject({ kind: "array", more: 800 });
    expect((f as { items: string[] }).items).toHaveLength(200);
  });

  test("an array of objects (or of arrays) falls back to JSON", () => {
    // Catches: rendering item objects as text "[object Object]".
    const s = {
      type: "object",
      properties: {
        w: { type: "array", items: { type: "object", properties: { a: { type: "number" } } } },
      },
    };
    const f = field(root(s, { w: [{ a: 1 }] }), "w");
    expect(f).toMatchObject({ kind: "json", reason: "unsupported" });
    expect((f as { json: string }).json).toContain('"a": 1');
  });

  test("a tuple (items as an array, or prefixItems) falls back to JSON", () => {
    const t1 = {
      type: "object",
      properties: { t: { type: "array", items: [{ type: "number" }, { type: "string" }] } },
    };
    const t2 = {
      type: "object",
      properties: { t: { type: "array", prefixItems: [{ type: "number" }] } },
    };
    expect(field(root(t1, { t: [1, "a"] }), "t")).toMatchObject({ kind: "json", reason: "tuple" });
    expect(field(root(t2, { t: [1] }), "t")).toMatchObject({ kind: "json", reason: "tuple" });
  });

  test("an item of the wrong primitive type makes the whole field JSON", () => {
    // Catches: showing "x" as a number.
    const f = field(root(radarSchema, { detonate_position: [1, "x", 3] }), "detonate_position");
    expect(f).toMatchObject({ kind: "json", reason: "type-mismatch" });
  });
});

describe("nested objects and $ref", () => {
  test("a $ref to #/$defs is resolved: children, title as label, nested required", () => {
    // Catches: leaving the ref unresolved or losing the target's title.
    const a = field(root(radarSchema, radarValue), "antenna");
    expect(a).toMatchObject({ kind: "object", label: "Antenna" });
    expect(kids(a).map((c) => c.key)).toEqual(["gain_db", "beam_width_deg", "mount"]);
    expect(field(a, "gain_db")).toMatchObject({ text: "30.5" });
    expect(field(a, "mount", "offset_m")).toMatchObject({ text: "1.5", unit: "m" });
  });

  test("a property title wins over the target title", () => {
    const s = {
      type: "object",
      properties: { a: { $ref: "#/$defs/x", title: "Mine" } },
      $defs: { x: { type: "object", title: "Theirs", properties: {} } },
    };
    expect(field(root(s, {}), "a").label).toBe("Mine");
  });

  test("#/definitions/... resolves like #/$defs/...", () => {
    // Catches: supporting only the 2020-12 spelling (ADR 0070 lists both).
    const s = {
      type: "object",
      properties: { a: { $ref: "#/definitions/n" } },
      definitions: { n: { type: "number" } },
    };
    expect(field(root(s, { a: 3 }), "a")).toMatchObject({ kind: "number", text: "3" });
  });

  test("a $ref at the root resolves to an object", () => {
    const s = {
      $ref: "#/$defs/p",
      $defs: { p: { type: "object", properties: { n: { type: "number" } } } },
    };
    expect(root(s, { n: 1 }).kind).toBe("object");
  });

  test("the same $ref used twice (non-recursive) resolves both times", () => {
    // Catches: treating a repeated, non-cyclic ref as recursion.
    const s = {
      type: "object",
      properties: { a: { $ref: "#/$defs/d" }, b: { $ref: "#/$defs/d" } },
      $defs: { d: { type: "number" } },
    };
    const r = root(s, { a: 1, b: 2 });
    expect([field(r, "a").kind, field(r, "b").kind]).toEqual(["number", "number"]);
  });

  test("a recursive $ref renders that subtree as JSON, 'recursive-ref'", () => {
    // Catches: infinite recursion; resolving only a bounded number of times and pretending it worked.
    const s = {
      type: "object",
      properties: { child: { $ref: "#/$defs/node" } },
      $defs: {
        node: {
          type: "object",
          properties: { child: { $ref: "#/$defs/node" }, n: { type: "number" } },
        },
      },
    };
    const r = root(s, { child: { n: 1, child: { n: 2 } } });
    const child = field(r, "child");
    expect(child.kind).toBe("object");
    expect(field(child, "child")).toMatchObject({ kind: "json", reason: "recursive-ref" });
    expect((field(child, "child") as { json: string }).json).toContain('"n": 2');
  });

  test("a self-referencing root ($ref: '#') is JSON, not a hang", () => {
    expect(root({ $ref: "#" }, { a: 1 })).toMatchObject({ kind: "json", reason: "recursive-ref" });
  });

  test("a missing, external or malformed $ref is JSON, 'unresolved-ref'", () => {
    // Catches: fetching/following external refs, throwing on a missing def.
    for (const ref of [
      "#/$defs/none",
      "http://example.com/s.json",
      "other.json#/x",
      "#/$defs/a/b/c",
      "",
      "#/$defs/__proto__",
    ]) {
      const s = { type: "object", properties: { a: { $ref: ref } } };
      expect(field(root(s, { a: 1 }), "a")).toMatchObject({
        kind: "json",
        reason: "unresolved-ref",
      });
    }
  });

  test("a non-string $ref is JSON, not a crash", () => {
    const s = { type: "object", properties: { a: { $ref: 5 } } };
    expect(field(root(s, { a: 1 }), "a").kind).toBe("json");
  });
});

describe("fallback to JSON: never silently dropped", () => {
  test.each([
    ["oneOf", { oneOf: [{ type: "number" }, { type: "string" }] }],
    ["anyOf", { anyOf: [{ type: "number" }, { type: "string" }] }],
    ["allOf", { allOf: [{ type: "object" }] }],
    ["if", JSON.parse('{"if":{"type":"number"},"then":{"minimum":1}}')],
    ["not", { not: { type: "number" } }],
    ["patternProperties", { type: "object", patternProperties: { "^x": { type: "number" } } }],
    ["const", { const: 5 }],
    ["type list", { type: ["string", "null"] }],
    ["no type", { description: "anything" }],
    ["unknown type", { type: "wibble" }],
    ["null type", { type: "null" }],
  ])("%s subtree becomes JSON with the note and keeps the value", (reason, schema) => {
    // Catches: ignoring the construct and rendering a plain field or dropping the value.
    const s = { type: "object", properties: { x: schema } };
    const f = field(root(s, { x: 7 }), "x");
    expect(f.kind).toBe("json");
    const j = f as Extract<FieldNode, { kind: "json" }>;
    expect(j.note).toBe("Shown as JSON");
    expect(j.json).toContain("7");
    expect(typeof j.reason).toBe("string");
    if (["oneOf", "anyOf", "allOf", "if", "not", "patternProperties"].includes(reason)) {
      expect(j.reason as string).toBe(reason);
    }
  });

  test("a JSON subtree shows the value, or the schema's default, or nothing-set text, never an empty drop", () => {
    // Catches: a json node whose `json` is '' when the value is absent. The text is the pretty value; for an absent value it is "(not set)".
    const s = { type: "object", properties: { x: { oneOf: [{ type: "number" }] } } };
    expect((field(root(s, {}), "x") as { json: string }).json).toBe("(not set)");
  });

  test("a non-object root falls back to JSON for the whole thing", () => {
    // Catches: assuming the root is an object (ADR 0070: a non-object root).
    for (const schema of [
      { type: "number" },
      { type: "array", items: { type: "number" } },
      { oneOf: [] },
      true,
      null,
      5,
      "x",
      [],
    ]) {
      expect(root(schema, 3).kind).toBe("json");
    }
  });

  test("a property schema that is not an object (true, null, a string) is JSON", () => {
    // Catches: `true` schemas crashing property access.
    const s = { type: "object", properties: { a: true, b: null, c: "number" } };
    const r = root(s, { a: 1, b: 2, c: 3 });
    expect(kids(r).map((c) => c.kind)).toEqual(["json", "json", "json"]);
  });

  test("a value of the wrong type for its field is shown as JSON with 'type-mismatch'", () => {
    // Catches: coercing "abc" to NaN text, or hiding the engine's actual value.
    const f = field(root(builtinSchema("builtin.visual"), { range_m: "far" }), "range_m");
    expect(f).toMatchObject({ kind: "json", reason: "type-mismatch" });
    expect((f as { json: string }).json).toBe('"far"');
    expect(field(root(radarSchema, { enabled: "yes" }), "enabled").kind).toBe("json");
    expect(field(root(radarSchema, { range_m: null }), "range_m").kind).toBe("json");
    expect(field(root(radarSchema, { channels: 2.5 }), "channels").kind).toBe("json");
  });

  test("a nested object schema given a non-object value is JSON", () => {
    expect(field(root(radarSchema, { antenna: [1] }), "antenna")).toMatchObject({
      kind: "json",
      reason: "type-mismatch",
    });
  });
});

describe("extra values: 'Other parameters'", () => {
  test("keys in the value but not in properties go to extras with pretty JSON, schema keys do not", () => {
    // Catches: dropping unknown keys (the engine accepted them), or duplicating known keys in extras.
    const r = root(builtinSchema("builtin.cond.chance"), {
      probability: 0.5,
      extra: { a: 1 },
      flag: true,
    });
    expect(r).toMatchObject({ kind: "object" });
    expect((r as { extras: unknown }).extras).toEqual([
      { key: "extra", json: JSON.stringify({ a: 1 }, null, 2) },
      { key: "flag", json: "true" },
    ]);
  });

  test("extras of nested objects stay with that object", () => {
    const a = field(root(radarSchema, { antenna: { gain_db: 1, spare: 2 } }), "antenna");
    expect((a as { extras: { key: string }[] }).extras.map((e) => e.key)).toEqual(["spare"]);
  });

  test("a non-object value for an object root is JSON (never an empty form)", () => {
    expect(root(builtinSchema("builtin.visual"), 5)).toMatchObject({
      kind: "json",
      reason: "type-mismatch",
    });
    expect(root(builtinSchema("builtin.visual"), [1])).toMatchObject({
      kind: "json",
      reason: "type-mismatch",
    });
  });

  test("undefined and null root values mean no parameters set", () => {
    // Catches: treating a missing value (template without params) as an error.
    expect(root(builtinSchema("builtin.visual"), undefined).kind).toBe("object");
    expect(root(builtinSchema("builtin.visual"), null).kind).toBe("object");
  });

  test("prototype-polluting keys are plain extras and never reach Object.prototype", () => {
    // Catches: assigning into a plain object by key, and `in` checks that see inherited keys.
    const value = JSON.parse('{"__proto__": {"x": 1}, "constructor": 2, "toString": 3}');
    const r = root(builtinSchema("builtin.cond.chance"), value);
    expect((r as { extras: { key: string }[] }).extras.map((e) => e.key).sort()).toEqual([
      "__proto__",
      "constructor",
      "toString",
    ]);
    expect(({} as Record<string, unknown>).x).toBeUndefined();
  });

  test("a schema property named like an Object.prototype member reads only own values", () => {
    // Catches: value["constructor"] returning Object for an absent key.
    const s = {
      type: "object",
      properties: { constructor: { type: "string" }, toString: { type: "number" } },
    };
    const r = root(s, {});
    expect(field(r, "constructor")).toMatchObject({ kind: "string", text: "", fromDefault: false });
    expect(field(r, "toString")).toMatchObject({ kind: "number", text: "" });
  });
});
