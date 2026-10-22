import { t } from "../messages.ts";
import { describeBounds } from "./describeBounds.ts";
import {
  BUDGET_KEY,
  CHIP_LIMIT,
  type ExtraEntry,
  type FieldBase,
  type FieldNode,
  type JsonReason,
  MAX_ARRAY_ITEMS,
  MAX_DEPTH,
  MAX_ENUM,
  MAX_NODES,
  MAX_TREE_JSON_CHARS,
} from "./fieldNode.ts";
import { safeStringify } from "./safeStringify.ts";
import { firstOwn, isPlain, own, type Plain, resolveRefs } from "./schemaAccess.ts";
import { unitFromKey } from "./unitFromKey.ts";

export type { ExtraEntry, FieldBase, FieldNode, JsonReason } from "./fieldNode.ts";

/** Keywords whose meaning this renderer does not interpret: the subtree is shown as JSON (ADR 0070). */
const UNINTERPRETED: readonly JsonReason[] = [
  "oneOf",
  "anyOf",
  "allOf",
  "if",
  "not",
  "patternProperties",
];
/** The budget node lists this many skipped properties with their values, then counts the rest. */
const MAX_SKIPPED_SHOWN = 100;
/** Characters of one skipped value in the budget node. */
const MAX_SKIPPED_VALUE_CHARS = 160;
/** Characters of a default shown in the help line of a JSON node. */
const MAX_DEFAULT_CHARS = 80;
const PRIMITIVES = new Set(["number", "integer", "string", "boolean"]);

type JsonNode = Extract<FieldNode, { kind: "json" }>;

interface Ctx {
  rootSchema: unknown;
  /** Field nodes created so far (the budget node is not counted). */
  count: number;
  budgetNode: JsonNode | undefined;
  /** What the budget node stands for: the first skipped properties by path, then a count. */
  skipped: Map<string, string>;
  skippedCount: number;
  lastSkipped: string;
  /** JSON characters produced so far (MAX_TREE_JSON_CHARS). */
  jsonChars: number;
}

interface Site {
  key: string;
  path: string[];
  required: boolean;
  depth: number;
  chain: ReadonlySet<string>;
  isRoot: boolean;
}

function jsonText(ctx: Ctx, value: unknown): string {
  if (value === undefined) return t("notSet");
  if (ctx.jsonChars >= MAX_TREE_JSON_CHARS) return t("jsonNotShown");
  const text = safeStringify(value);
  ctx.jsonChars += text.length;
  return text;
}

/** Short text of a default for the help line; long or structured defaults are cut. */
function shortDefault(d: unknown): string | undefined {
  if (d === undefined) return undefined;
  const text = typeof d === "object" && d !== null ? safeStringify(d) : String(d);
  return text.length > MAX_DEFAULT_CHARS ? `${text.slice(0, MAX_DEFAULT_CHARS)}…` : text;
}

function baseOf(site: Site, seen: readonly unknown[]): FieldBase {
  const title = firstOwn(seen, "title");
  const description = firstOwn(seen, "description");
  return {
    key: site.key,
    path: site.path,
    label: typeof title === "string" && title !== "" ? title : site.key,
    ...(typeof description === "string" && description !== "" ? { description } : {}),
    required: site.required,
  };
}

function jsonNode(
  ctx: Ctx,
  site: Site,
  seen: readonly unknown[],
  value: unknown,
  reason: JsonReason,
): JsonNode {
  ctx.count += 1;
  const defaultText = shortDefault(defaultValue(seen));
  return {
    ...baseOf(site, seen),
    kind: "json",
    json: jsonText(ctx, value),
    note: t("shownAsJson"),
    reason,
    ...(defaultText !== undefined ? { defaultText } : {}),
    fromDefault: value === undefined && defaultText !== undefined,
  };
}

function defaultValue(seen: readonly unknown[]): unknown {
  return firstOwn(seen, "default");
}

function typedDefault(
  seen: readonly unknown[],
  type: "number" | "string" | "boolean",
): string | undefined {
  const d = defaultValue(seen);
  return typeof d === type && (type !== "number" || Number.isFinite(d)) ? String(d) : undefined;
}

/** The string entries of `required`, built once per object. */
function requiredKeys(schema: Plain): ReadonlySet<string> {
  const req = own(schema, "required");
  const out = new Set<string>();
  if (!Array.isArray(req)) return out;
  for (let i = 0; i < req.length; i++) if (typeof req[i] === "string") out.add(req[i] as string);
  return out;
}

function build(ctx: Ctx, schema: unknown, value: unknown, site: Site): FieldNode {
  try {
    return buildUnchecked(ctx, schema, value, site);
  } catch {
    // Defence in depth: buildChild and the top level also guard, but a missed read must never escape.
    // Property access on a hostile schema or value (getter, Proxy) threw: show the subtree as JSON.
    return jsonNode(ctx, site, [], value, "unsupported");
  }
}

function buildUnchecked(ctx: Ctx, schema: unknown, value: unknown, outer: Site): FieldNode {
  const resolved = resolveRefs(schema, ctx.rootSchema, outer.chain);
  if (!resolved.ok) return jsonNode(ctx, outer, resolved.seen, value, resolved.reason);
  const { seen } = resolved;
  const s = resolved.schema;
  const site: Site = { ...outer, chain: resolved.chain };
  if (!isPlain(s)) return jsonNode(ctx, site, seen, value, "unsupported");
  for (const keyword of UNINTERPRETED) {
    if (Object.hasOwn(s, keyword)) return jsonNode(ctx, site, seen, value, keyword);
  }
  if (Object.hasOwn(s, "const")) return jsonNode(ctx, site, seen, value, "unsupported");
  const type = own(s, "type");
  if (
    typeof type !== "string" ||
    !(type === "object" || PRIMITIVES.has(type) || type === "array")
  ) {
    return jsonNode(ctx, site, seen, value, "unsupported");
  }
  if (site.isRoot && type !== "object") return jsonNode(ctx, site, seen, value, "unsupported");
  if (type !== "string" && Object.hasOwn(s, "enum")) {
    const list = s.enum;
    const huge = Array.isArray(list) && list.length > MAX_ENUM;
    return jsonNode(ctx, site, seen, value, huge ? "enum-too-large" : "unsupported");
  }
  const base = baseOf(site, seen);
  const mismatch = () => jsonNode(ctx, site, seen, value, "type-mismatch");
  switch (type) {
    case "object":
      return buildObject(ctx, s, seen, value, site, base);
    case "number":
    case "integer": {
      const ok = type === "integer" ? Number.isInteger(value) : Number.isFinite(value);
      if (value !== undefined && !ok) return mismatch();
      ctx.count += 1;
      const defaultText = typedDefault(seen, "number");
      const unit = unitFromKey(site.key);
      const bounds = describeBounds(s);
      const fromDefault = value === undefined && defaultText !== undefined;
      return {
        ...base,
        kind: type,
        text: value === undefined ? (defaultText ?? "") : String(value),
        ...(unit ? { unit } : {}),
        ...(bounds ? { bounds } : {}),
        ...(defaultText !== undefined ? { defaultText } : {}),
        fromDefault,
      };
    }
    case "string":
      return buildString(ctx, s, seen, value, site, base);
    case "boolean": {
      if (value !== undefined && typeof value !== "boolean") return mismatch();
      ctx.count += 1;
      const defaultText = typedDefault(seen, "boolean");
      const fromDefault = value === undefined && defaultText !== undefined;
      return {
        ...base,
        kind: "boolean",
        checked: value === undefined ? defaultText === "true" : value,
        ...(defaultText !== undefined ? { defaultText } : {}),
        fromDefault,
      };
    }
    default:
      return buildArray(ctx, s, seen, value, site, base);
  }
}

function buildString(
  ctx: Ctx,
  s: Plain,
  seen: readonly unknown[],
  value: unknown,
  site: Site,
  base: FieldBase,
): FieldNode {
  if (Object.hasOwn(s, "enum")) {
    const list = s.enum;
    if (Array.isArray(list) && list.length > MAX_ENUM) {
      return jsonNode(ctx, site, seen, value, "enum-too-large");
    }
    if (!Array.isArray(list) || list.length === 0 || !list.every((o) => typeof o === "string")) {
      return jsonNode(ctx, site, seen, value, "unsupported");
    }
    const options = [...new Set<string>(list)];
    const d = typedDefault(seen, "string");
    const defaultText = d !== undefined && options.includes(d) ? d : undefined;
    if (value !== undefined && !(typeof value === "string" && options.includes(value))) {
      return jsonNode(ctx, site, seen, value, "type-mismatch");
    }
    ctx.count += 1;
    return {
      ...base,
      kind: "enum",
      options,
      selected: value === undefined ? defaultText : (value as string),
      control: options.length <= CHIP_LIMIT ? "chips" : "select",
      ...(defaultText !== undefined ? { defaultText } : {}),
      fromDefault: value === undefined && defaultText !== undefined,
    };
  }
  if (value !== undefined && typeof value !== "string") {
    return jsonNode(ctx, site, seen, value, "type-mismatch");
  }
  ctx.count += 1;
  const defaultText = typedDefault(seen, "string");
  return {
    ...base,
    kind: "string",
    text: value === undefined ? (defaultText ?? "") : value,
    ...(defaultText !== undefined ? { defaultText } : {}),
    fromDefault: value === undefined && defaultText !== undefined,
  };
}

function primitiveItem(
  ctx: Ctx,
  items: unknown,
): { type: "number" | "integer" | "string" | "boolean" } | { reason: JsonReason } {
  const r = resolveRefs(items, ctx.rootSchema, new Set());
  if (!r.ok) return { reason: r.reason };
  const s = r.schema;
  if (!isPlain(s) || Object.hasOwn(s, "enum") || Object.hasOwn(s, "const")) {
    return { reason: "unsupported" };
  }
  for (const keyword of UNINTERPRETED)
    if (Object.hasOwn(s, keyword)) return { reason: "unsupported" };
  const t = own(s, "type");
  return t === "number" || t === "integer" || t === "string" || t === "boolean"
    ? { type: t }
    : { reason: "unsupported" };
}

function itemMatches(type: "number" | "integer" | "string" | "boolean", item: unknown): boolean {
  if (type === "integer") return Number.isInteger(item);
  if (type === "number") return typeof item === "number" && Number.isFinite(item);
  return typeof item === type;
}

function allMatch(
  type: "number" | "integer" | "string" | "boolean",
  list: readonly unknown[],
): boolean {
  for (let i = 0; i < list.length; i++) if (!itemMatches(type, list[i])) return false;
  return true;
}

function buildArray(
  ctx: Ctx,
  s: Plain,
  seen: readonly unknown[],
  value: unknown,
  site: Site,
  base: FieldBase,
): FieldNode {
  if (Array.isArray(s.items) || Object.hasOwn(s, "prefixItems")) {
    return jsonNode(ctx, site, seen, value, "tuple");
  }
  const item = Object.hasOwn(s, "items")
    ? primitiveItem(ctx, s.items)
    : { reason: "unsupported" as const };
  if ("reason" in item) return jsonNode(ctx, site, seen, value, item.reason);
  const raw = defaultValue(seen);
  const fallback = Array.isArray(raw) && allMatch(item.type, raw) ? raw : undefined;
  if (value !== undefined && !(Array.isArray(value) && allMatch(item.type, value))) {
    return jsonNode(ctx, site, seen, value, "type-mismatch");
  }
  const list: unknown[] = value === undefined ? (fallback ?? []) : (value as unknown[]);
  const fromDefault = value === undefined && fallback !== undefined;
  ctx.count += 1;
  const shown = list.slice(0, MAX_ARRAY_ITEMS).map((x) => String(x));
  const unit = unitFromKey(site.key);
  const defaultText = fallback
    ?.slice(0, MAX_ARRAY_ITEMS)
    .map((x) => String(x))
    .join(", ");
  return {
    ...base,
    kind: "array",
    items: shown,
    more: Math.max(0, list.length - shown.length),
    ...(unit ? { unit } : {}),
    ...(defaultText !== undefined ? { defaultText } : {}),
    fromDefault,
  };
}

function skip(ctx: Ctx, site: Site, children: FieldNode[], key: string, value: unknown): void {
  const label = [...site.path, key].join(".");
  ctx.skippedCount += 1;
  ctx.lastSkipped = label;
  if (ctx.skippedCount <= MAX_SKIPPED_SHOWN) {
    let slot = label;
    for (let n = 2; ctx.skipped.has(slot); n++) slot = `${label}#${n}`;
    ctx.skipped.set(
      slot,
      value === undefined ? t("notSet") : safeStringify(value).slice(0, MAX_SKIPPED_VALUE_CHARS),
    );
  }
  if (!ctx.budgetNode) {
    ctx.budgetNode = {
      key: BUDGET_KEY,
      path: [...site.path, BUDGET_KEY],
      label: "…",
      required: false,
      kind: "json",
      json: "",
      note: t("shownAsJson"),
      reason: "budget",
      fromDefault: false,
    };
    children.push(ctx.budgetNode);
  }
}

/** The budget node's text: the count first (it must survive the length cut), then the first skipped fields. */
function budgetJson(ctx: Ctx): string {
  const out: Record<string, string> = Object.create(null);
  const more = ctx.skippedCount - MAX_SKIPPED_SHOWN;
  if (more > 0) out["…"] = t("moreFields", { count: more, last: ctx.lastSkipped });
  for (const [slot, text] of ctx.skipped) {
    out[Object.hasOwn(out, slot) ? `${slot}#` : slot] = text;
  }
  return safeStringify(out);
}

function readSafe(read: () => unknown): unknown {
  try {
    return read();
  } catch {
    return t("unreadable");
  }
}

function buildChild(ctx: Ctx, props: Plain, key: string, value: unknown, site: Site): FieldNode {
  try {
    return build(ctx, props[key], value, site);
  } catch {
    // Reading the property schema itself threw (a getter): only this field becomes JSON.
    return jsonNode(ctx, site, [], value, "unsupported");
  }
}

function buildObject(
  ctx: Ctx,
  s: Plain,
  seen: readonly unknown[],
  value: unknown,
  site: Site,
  base: FieldBase,
): FieldNode {
  const given = site.isRoot && value === null ? undefined : value;
  if (given !== undefined && !isPlain(given)) {
    return jsonNode(ctx, site, seen, value, "type-mismatch");
  }
  if (site.depth > MAX_DEPTH) return jsonNode(ctx, site, seen, value, "depth");
  const props = Object.hasOwn(s, "properties") ? s.properties : {};
  if (!isPlain(props)) return jsonNode(ctx, site, seen, value, "unsupported");
  ctx.count += 1;
  const record = given as Plain | undefined;
  const children: FieldNode[] = [];
  const known = new Set<string>();
  const required = requiredKeys(s);
  for (const key of Object.keys(props)) {
    known.add(key);
    const childValue = record ? readSafe(() => own(record, key)) : undefined;
    if (ctx.count >= MAX_NODES) {
      skip(ctx, site, children, key, childValue);
      continue;
    }
    children.push(
      buildChild(ctx, props, key, childValue, {
        key,
        path: [...site.path, key],
        required: required.has(key),
        depth: site.depth + 1,
        chain: site.chain,
        isRoot: false,
      }),
    );
  }
  return { ...base, kind: "object", children, extras: record ? extrasOf(ctx, record, known) : [] };
}

/** Values the schema does not list. A hostile `ownKeys` costs only this list, not the fields. */
function extrasOf(ctx: Ctx, record: Plain, known: ReadonlySet<string>): ExtraEntry[] {
  let keys: string[];
  try {
    keys = Object.keys(record).filter((k) => !known.has(k));
  } catch {
    return [{ key: "…", json: t("unreadable"), summary: true }];
  }
  const room = Math.max(0, MAX_NODES - ctx.count);
  const extras: ExtraEntry[] = [];
  for (const key of keys.slice(0, room)) {
    extras.push({
      key,
      json: jsonText(
        ctx,
        readSafe(() => record[key]),
      ),
    });
  }
  ctx.count += extras.length;
  if (keys.length > room) {
    extras.push({ key: "…", json: t("moreKeys", { count: keys.length - room }), summary: true });
  }
  return extras;
}

/**
 * Turns a JSON Schema (subset, ADR 0070) and its value into a render tree. Never throws: whatever the
 * subset cannot show becomes a JSON node and nothing is dropped. Pure and deterministic.
 */
export function toFieldTree(schema: unknown, value: unknown): { root: FieldNode } {
  const ctx: Ctx = {
    rootSchema: schema,
    count: 0,
    budgetNode: undefined,
    skipped: new Map(),
    skippedCount: 0,
    lastSkipped: "",
    jsonChars: 0,
  };
  const site: Site = {
    key: "",
    path: [],
    required: false,
    depth: 0,
    chain: new Set(),
    isRoot: true,
  };
  let root: FieldNode;
  try {
    root = build(ctx, schema, value, site);
    if (ctx.budgetNode) ctx.budgetNode.json = budgetJson(ctx);
  } catch {
    root = {
      key: "",
      path: [],
      label: "",
      required: false,
      kind: "json",
      json: jsonText(ctx, value),
      note: t("shownAsJson"),
      reason: "unsupported",
      fromDefault: false,
    };
  }
  return { root };
}
