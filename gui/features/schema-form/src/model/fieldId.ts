const SAFE = /[A-Za-z0-9_]/;

/** One path segment as id-safe text; "." and "-" never survive, so segments cannot run into each other. */
function escapeSegment(segment: string): string {
  let out = "";
  for (const ch of segment) {
    out += SAFE.test(ch) ? ch : `~${(ch.codePointAt(0) ?? 0).toString(16)}~`;
  }
  return out;
}

/** `${idPrefix}-` plus the escaped property path, unique per field within a form. */
export function fieldId(idPrefix: string, path: readonly string[]): string {
  return `${idPrefix}-${path.map(escapeSegment).join(".")}`;
}
