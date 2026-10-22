import { t } from "../messages.ts";

export const MAX_JSON_CHARS = 20_000;
const MAX_DEPTH = 32;
const MAX_VISITS = 10_000;

interface Walk {
  ancestors: object[];
  visits: number;
}

function plain(value: unknown, depth: number, walk: Walk): unknown {
  if (typeof value === "bigint") return value.toString();
  if (typeof value === "number") return Number.isFinite(value) ? value : String(value);
  if (typeof value === "function") return t("functionValue");
  if (typeof value === "symbol") return t("symbolValue");
  if (value === undefined) return t("undefinedValue");
  if (value === null || typeof value !== "object") return value;
  if (walk.ancestors.includes(value)) return t("circular");
  if (depth >= MAX_DEPTH) return t("tooDeep");
  if (walk.visits >= MAX_VISITS) return "…";
  walk.visits += 1;
  walk.ancestors.push(value);
  try {
    if (value instanceof Date) {
      return Number.isNaN(value.getTime()) ? t("invalidDate") : value.toISOString();
    }
    if (Array.isArray(value)) {
      const out: unknown[] = [];
      const length = Math.min(value.length, 1000);
      for (let i = 0; i < length; i++) out.push(read(() => value[i], depth, walk));
      if (value.length > length) out.push(t("moreItems", { count: value.length - length }));
      return out;
    }
    const out: Record<string, unknown> = Object.create(null);
    const record = value as Record<string, unknown>;
    const keys = Object.keys(value);
    for (const key of keys.slice(0, 1000)) out[key] = read(() => record[key], depth, walk);
    if (keys.length > 1000) out["…"] = t("moreKeys", { count: keys.length - 1000 });
    return out;
  } catch {
    return t("unreadable");
  } finally {
    walk.ancestors.pop();
  }
}

function read(get: () => unknown, depth: number, walk: Walk): unknown {
  try {
    return plain(get(), depth + 1, walk);
  } catch {
    return t("unreadable");
  }
}

/**
 * Pretty JSON text of any value, for the read-only JSON fallback view. Never throws: cycles print
 * "[Circular]", values deeper than 32 levels "[Too deep]", bigint its decimal string, and the output is
 * cut at 20,000 characters.
 */
export function safeStringify(value: unknown): string {
  try {
    const walk: Walk = { ancestors: [], visits: 0 };
    const tree = plain(value, 0, walk);
    const text =
      value === undefined || typeof value === "function" || typeof value === "symbol"
        ? String(tree)
        : JSON.stringify(tree, null, 2);
    return text.length > MAX_JSON_CHARS ? text.slice(0, MAX_JSON_CHARS) + t("truncated") : text;
  } catch {
    return t("unreadable");
  }
}
