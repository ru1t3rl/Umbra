import type { JournalRecord } from "@umbra/engine-stream";

type Fields = Readonly<Record<string, unknown>>;

const fields = (value: unknown): Fields =>
  typeof value === "object" && value !== null ? (value as Fields) : {};
const text = (value: unknown): string | undefined =>
  typeof value === "string" && value !== "" ? value : undefined;
const id = (value: unknown): string | undefined =>
  typeof value === "number" && Number.isFinite(value) ? `#${value}` : undefined;
const join = (...parts: readonly (string | undefined)[]): string =>
  parts.filter((p): p is string => p !== undefined).join(" ");
const paren = (value: string | undefined): string | undefined =>
  value === undefined ? undefined : `(${value})`;

/**
 * The one line a record shows. Event records are formatted from the event DTO the journal keeps as `detail`
 * (an unknown kind verbatim, forward compatible); every other source, and the loss records, show `message`.
 * Never prints undefined, null or NaN (ASM-944 a).
 */
export function formatRecord(record: JournalRecord): string {
  if (
    record.source !== "event" ||
    record.kind === "events_truncated" ||
    record.kind === "frames_dropped"
  )
    return record.message;
  const dto = fields(record.detail);
  const inner = fields(dto.detail);
  const subject = id(record.entity_id);
  const target = id(dto.target_id);
  switch (record.kind) {
    case "entity_created":
      return join("Created", subject ?? "entity", paren(text(dto.template)));
    case "entity_removed":
      return join("Removed", subject ?? "entity");
    case "fire":
      if (subject === undefined) return "Fire";
      return join(subject, "fired", target && `at ${target}`, paren(text(dto.weapon)));
    case "detonation":
      if (subject === undefined) return "Detonation";
      return join(subject, "detonated", target && `at ${target}`);
    case "simulation_control": {
      const action = text(inner.action);
      const count = typeof inner.count === "number" ? String(inner.count) : undefined;
      return join("Simulation", action ?? "control", action === undefined ? undefined : count);
    }
    case "custom":
      return text(inner.kind) === undefined ? "custom" : `custom: ${text(inner.kind)}`;
    default:
      return record.kind;
  }
}
