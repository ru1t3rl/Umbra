import type { Severity } from "@umbra/data";

/** A location as the user reads it: `file:line:column`, the missing parts left out (null when no file). */
export interface DiagnosticView {
  code: string;
  message: string;
  location: string | null;
  /** A missing severity reads `error` (API 1.1.0). */
  severity: Severity;
  notes: readonly { message: string; location: string | null }[];
}

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);

const text = (v: unknown): string | null => (typeof v === "string" && v !== "" ? v : null);
const count = (v: unknown): number | null =>
  typeof v === "number" && Number.isInteger(v) && v > 0 ? v : null;

/** `file:line:column`; a line needs a file, a column needs a line; nothing without a file. */
export function formatLocation(file: unknown, line: unknown, column: unknown): string | null {
  const f = text(file);
  if (f === null) return null;
  const l = count(line);
  if (l === null) return f;
  const c = count(column);
  return c === null ? `${f}:${l}` : `${f}:${l}:${c}`;
}

/** How many diagnostics the list shows; the rest is counted (a hostile scenario can have thousands). */
export const MAX_DIAGNOSTICS = 50;

/** Decode problem or warning diagnostics defensively, in server order; entries that are not diagnostics are dropped. */
export function toDiagnosticViews(raw: unknown): DiagnosticView[] {
  const views: DiagnosticView[] = [];
  for (const item of Array.isArray(raw) ? raw : []) {
    if (!isRecord(item) || typeof item.code !== "string" || typeof item.message !== "string") {
      continue;
    }
    const notes = Array.isArray(item.notes) ? item.notes : [];
    views.push({
      code: item.code,
      message: item.message,
      location: formatLocation(item.file, item.line, item.column),
      severity: item.severity === "warning" ? "warning" : "error",
      notes: notes.flatMap((n: unknown) =>
        isRecord(n) && typeof n.message === "string"
          ? [{ message: n.message, location: formatLocation(n.file, n.line, n.column) }]
          : [],
      ),
    });
  }
  return views;
}
