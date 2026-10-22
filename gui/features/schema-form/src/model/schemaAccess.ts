/** Guarded reads of untrusted schema and value objects (own properties only, no prototype pitfalls). */

export type Plain = Record<string, unknown>;

/** A JSON-like object: null or Object.prototype prototype, so Date, Map and class instances are not records. */
export function isPlain(x: unknown): x is Plain {
  if (typeof x !== "object" || x === null || Array.isArray(x)) return false;
  const proto = Object.getPrototypeOf(x);
  return proto === null || proto === Object.prototype;
}

export function own(obj: Plain, key: string): unknown {
  return Object.hasOwn(obj, key) ? obj[key] : undefined;
}

export type RefResult =
  | { ok: true; schema: unknown; chain: ReadonlySet<string>; seen: unknown[] }
  | { ok: false; reason: "recursive-ref" | "unresolved-ref" | "ref-chain"; seen: unknown[] };

/** A `$ref` that points at another `$ref` this many times in a row is JSON (reason `ref-chain`). */
const MAX_REF_HOPS = 32;
const REF = /^#\/(\$defs|definitions)\/([^/~]+)$/;

/**
 * Follows `$ref` (only `#/$defs/<name>` and `#/definitions/<name>`) until a schema without one. A ref
 * that is already on `chain` (its own ancestors) is recursive. `seen` lists every schema visited, first
 * the starting one, so annotations (title, description, default) can fall back along the chain.
 */
export function resolveRefs(
  schema: unknown,
  rootSchema: unknown,
  chain: ReadonlySet<string>,
): RefResult {
  let current = schema;
  let active = chain;
  let owned = false;
  const seen: unknown[] = [schema];
  for (let hops = 0; ; hops++) {
    if (hops > MAX_REF_HOPS) return { ok: false, reason: "ref-chain", seen };
    if (!isPlain(current) || !Object.hasOwn(current, "$ref")) {
      return { ok: true, schema: current, chain: active, seen };
    }
    const ref = current.$ref;
    if (typeof ref !== "string") return { ok: false, reason: "unresolved-ref", seen };
    if (ref === "#" || active.has(ref)) return { ok: false, reason: "recursive-ref", seen };
    const match = REF.exec(ref);
    const name = match?.[2];
    if (!match || name === undefined || name === "__proto__" || !isPlain(rootSchema)) {
      return { ok: false, reason: "unresolved-ref", seen };
    }
    const defs = own(rootSchema, match[1] === "$defs" ? "$defs" : "definitions");
    if (!isPlain(defs) || !Object.hasOwn(defs, name)) {
      return { ok: false, reason: "unresolved-ref", seen };
    }
    if (!owned) {
      active = new Set(active);
      owned = true;
    }
    (active as Set<string>).add(ref);
    current = defs[name];
    seen.push(current);
  }
}

/** First own value of `key` along the visited schemas, e.g. a property title before its target's title. */
export function firstOwn(seen: readonly unknown[], key: string): unknown {
  for (const s of seen) {
    if (isPlain(s) && Object.hasOwn(s, key)) return s[key];
  }
  return undefined;
}
