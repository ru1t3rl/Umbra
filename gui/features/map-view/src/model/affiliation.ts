import type { Affiliation } from "@umbra/ui-kit";

/** Default force-to-affiliation mapping (ASM-305, ADR 0042). Keys are lower case. */
const BY_FORCE: Readonly<Record<string, Affiliation>> = {
  blue: "friend",
  friend: "friend",
  friendly: "friend",
  red: "hostile",
  hostile: "hostile",
  opfor: "hostile",
  green: "neutral",
  neutral: "neutral",
  white: "neutral",
};

/** `force` is free text in the contract. Case-insensitive; null, undefined or anything else is "unknown". */
export function affiliationOf(force: string | null | undefined): Affiliation {
  if (typeof force !== "string") return "unknown";
  // Own-keys lookup: "constructor" or "__proto__" must not resolve through the prototype.
  const key = force.trim().toLowerCase();
  return Object.hasOwn(BY_FORCE, key) ? (BY_FORCE[key] ?? "unknown") : "unknown";
}
