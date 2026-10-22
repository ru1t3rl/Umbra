import type { FieldNode } from "./fieldNode.ts";

export interface HelpMessages {
  required: string;
  unit: (unit: string) => string;
  default: (value: string) => string;
  usingDefault: (value: string) => string;
}

/**
 * Help text parts in a fixed order: description, Required, Unit, bounds, then "Default: d" for a given
 * value or "Using default: d" for an absent one. Joined with " · ". Undefined when there is nothing.
 */
export function helpText(node: FieldNode, m: HelpMessages): string | undefined {
  if (node.kind === "object") return node.description;
  const parts: string[] = [];
  if (node.description) parts.push(node.description);
  if (node.required) parts.push(m.required);
  if ((node.kind === "number" || node.kind === "integer" || node.kind === "array") && node.unit) {
    parts.push(m.unit(node.unit));
  }
  if ((node.kind === "number" || node.kind === "integer") && node.bounds) parts.push(node.bounds);
  if (node.defaultText !== undefined) {
    parts.push(node.fromDefault ? m.usingDefault(node.defaultText) : m.default(node.defaultText));
  }
  return parts.length > 0 ? parts.join(" · ") : undefined;
}
