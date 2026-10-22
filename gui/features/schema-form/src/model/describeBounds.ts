import { t } from "../messages.ts";

const PARTS = [
  ["minimum", "boundMin"],
  ["exclusiveMinimum", "boundGreaterThan"],
  ["maximum", "boundMax"],
  ["exclusiveMaximum", "boundLessThan"],
] as const;

/** Numeric bounds as help text, e.g. "greater than 0, max 1000". Non-numeric bounds are ignored. */
export function describeBounds(schema: Record<string, unknown>): string | undefined {
  const out: string[] = [];
  for (const [keyword, message] of PARTS) {
    const bound = Object.hasOwn(schema, keyword) ? schema[keyword] : undefined;
    if (typeof bound === "number" && Number.isFinite(bound)) out.push(t(message, { n: bound }));
  }
  return out.length > 0 ? out.join(", ") : undefined;
}
