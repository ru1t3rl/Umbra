// Decoder for stream messages (ADR 0027/0045). Input is parsed into `unknown` and narrowed by
// type guards; nothing is cast. Unknown message types, unknown enum values and unknown fields are
// tolerated (docs/api.md, "clients must ignore unknown fields, enum values and message types").
import type { Hello } from "../generated/stream";
import type { Clock, EntityFrame, EntitySummary, EventSummary, RunStatus } from "../types.ts";

export const STREAM_PROTOCOL = "umbra.stream.v1";

export type Decoded =
  | { kind: "hello"; hello: Hello }
  | {
      kind: "frame";
      frame: EntityFrame;
      run: RunStatus | undefined;
      skipped: number;
      /** The frame's events in order, for the journal (ADR 0071); never stored in the entity store. */
      events: readonly EventSummary[];
      events_truncated: boolean;
    }
  | { kind: "error"; code: string; message: string }
  /** Not understood (bad JSON, unknown `type`, invalid shape); counted, never applied. */
  | { kind: "ignored"; reason: string };

/** A decoder turns one raw socket message into a `Decoded`. Only `json` exists in M5b. */
export type Decoder = (raw: string) => Decoded;

type Rec = Record<string, unknown>;
type Mutable<T> = { -readonly [K in keyof T]: T[K] };
const isRec = (v: unknown): v is Rec => typeof v === "object" && v !== null && !Array.isArray(v);
const isNum = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);
const isStr = (v: unknown): v is string => typeof v === "string";
const isOptStr = (v: unknown): v is string | null | undefined =>
  v === undefined || v === null || typeof v === "string";
const isOptRoute = (v: unknown): boolean =>
  v === undefined || v === null || (isRec(v) && isNum(v.len) && isNum(v.next));
const OWNERSHIPS = ["owned", "remote", "unknown"] as const;
const STATES = ["idle", "ready", "running", "paused", "stopped"] as const;
const STOP_REASONS = ["command", "duration", "plugin_fault", "network_fault", "error"] as const;
const RATE_SOURCES = ["scenario", "default"] as const;

function oneOf<T extends string>(values: readonly T[], v: unknown): T | undefined {
  return values.find((x) => x === v);
}

function isEntitySummary(v: Rec): v is Rec & EntitySummary {
  return (
    isNum(v.id) &&
    isNum(v.lat_deg) &&
    isNum(v.lon_deg) &&
    isNum(v.alt_m) &&
    isNum(v.heading_deg) &&
    isNum(v.speed_mps) &&
    oneOf(OWNERSHIPS, v.ownership) !== undefined &&
    isOptRoute(v.route) &&
    isOptStr(v.name) &&
    isOptStr(v.template) &&
    isOptStr(v.force) &&
    isOptStr(v.group) &&
    isOptStr(v.behavior) &&
    isOptStr(v.status)
  );
}

/**
 * A valid entity as parsed (no copy), a repaired copy when only `ownership` (unknown, missing or
 * mistyped -> "unknown") or `route` (malformed -> null) is bad, or undefined (skipped and counted) when a
 * required number is missing or mistyped.
 */
function entityOf(v: unknown): EntitySummary | undefined {
  if (!isRec(v)) return undefined;
  if (isEntitySummary(v)) return v;
  const repaired = {
    ...v,
    ownership: oneOf(OWNERSHIPS, v.ownership) ?? "unknown",
    route: isOptRoute(v.route) ? v.route : null,
  };
  return isEntitySummary(repaired) ? repaired : undefined;
}

function pluginFault(v: unknown): RunStatus["fault"] {
  if (!isRec(v)) return null;
  if (!isStr(v.call) || !isStr(v.capability) || !isStr(v.message) || !isStr(v.plugin)) return null;
  return { call: v.call, capability: v.capability, message: v.message, plugin: v.plugin };
}

function clockOf(v: unknown): Clock {
  if (!isRec(v)) return { mode: "unknown" };
  switch (v.mode) {
    case "afap":
      return { mode: "afap" };
    case "realtime":
      return { mode: "realtime" };
    case "stepped":
      return { mode: "stepped" };
    case "scaled":
      return isNum(v.scale) ? { mode: "scaled", scale: v.scale } : { mode: "unknown" };
    default:
      return { mode: "unknown" }; // a clock mode from a newer minor version
  }
}

/**
 * The run status. An unknown `state` or clock `mode` becomes "unknown" and everything else, `tick` and
 * `time_s` included, is kept (ASM-359). Undefined only when `session` is missing (not a run status).
 */
export function runOf(v: unknown): RunStatus | undefined {
  if (!isRec(v) || !isNum(v.session)) return undefined;
  const run: Mutable<RunStatus> = {
    clock: clockOf(v.clock),
    session: v.session,
    state: oneOf(STATES, v.state) ?? "unknown",
    step_pending: v.step_pending === true,
  };
  if (isNum(v.tick)) run.tick = v.tick;
  if (isNum(v.time_s)) run.time_s = v.time_s;
  if (isNum(v.tick_rate_hz)) run.tick_rate_hz = v.tick_rate_hz;
  const source = oneOf(RATE_SOURCES, v.tick_rate_source);
  if (source !== undefined) run.tick_rate_source = source;
  const reason = oneOf(STOP_REASONS, v.stop_reason);
  if (reason !== undefined) run.stop_reason = reason;
  else if (v.stop_reason !== undefined && v.stop_reason !== null) run.stop_reason = "unknown"; // a reason this client does not know (ASM-359)
  const fault = pluginFault(v.fault);
  if (fault) run.fault = fault;
  return run;
}

function helloOf(v: Rec): Hello | undefined {
  const s = v.settings;
  if (!isStr(v.protocol) || !isStr(v.api_version) || !isStr(v.encoding) || !isRec(s)) {
    return undefined;
  }
  const num = (k: string): number | undefined => (isNum(s[k]) ? s[k] : undefined);
  const settings = {
    default_rate_hz: num("default_rate_hz"),
    max_entity_ids: num("max_entity_ids"),
    max_events_per_frame: num("max_events_per_frame"),
    max_message_bytes: num("max_message_bytes"),
    max_rate_hz: num("max_rate_hz"),
    queue_frames: num("queue_frames"),
    send_timeout_s: num("send_timeout_s"),
  };
  const { default_rate_hz, max_rate_hz } = settings;
  if (default_rate_hz === undefined || max_rate_hz === undefined) return undefined;
  return {
    protocol: v.protocol,
    api_version: v.api_version,
    encoding: v.encoding,
    settings: {
      default_rate_hz,
      max_rate_hz,
      max_entity_ids: settings.max_entity_ids ?? 0,
      max_events_per_frame: settings.max_events_per_frame ?? 0,
      max_message_bytes: settings.max_message_bytes ?? 0,
      queue_frames: settings.queue_frames ?? 0,
      send_timeout_s: settings.send_timeout_s ?? 0,
    },
  };
}

function eventOf(v: unknown): EventSummary | undefined {
  return isRec(v) && isStr(v.kind) && isNum(v.tick) ? (v as EventSummary) : undefined;
}

function eventsOf(v: unknown): EventSummary[] {
  if (!Array.isArray(v)) return [];
  const events: EventSummary[] = [];
  for (const raw of v as readonly unknown[]) {
    const e = eventOf(raw);
    if (e) events.push(e);
  }
  return events;
}

function frameOf(v: Rec): Decoded {
  if (
    !isNum(v.session) ||
    !isNum(v.seq) ||
    !isNum(v.tick) ||
    !isNum(v.time_s) ||
    !Array.isArray(v.entities)
  ) {
    return { kind: "ignored", reason: "invalid frame" };
  }
  // `cause` ("tick" | "control", or a later value) is not needed by the GUI and is not read.
  const entities: EntitySummary[] = [];
  let skipped = 0;
  for (const raw of v.entities as readonly unknown[]) {
    const e = entityOf(raw);
    if (e) entities.push(e);
    else skipped += 1;
  }
  const frame: EntityFrame = {
    session: v.session,
    seq: v.seq,
    tick: v.tick,
    time_s: v.time_s,
    dropped: isNum(v.dropped) ? v.dropped : 0,
    entities: Object.freeze(entities),
  };
  return {
    kind: "frame",
    frame,
    run: runOf(v.run),
    skipped,
    events: eventsOf(v.events),
    events_truncated: v.events_truncated === true,
  };
}

export const decodeJson: Decoder = (raw) => {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return { kind: "ignored", reason: "invalid JSON" };
  }
  if (!isRec(parsed)) return { kind: "ignored", reason: "not an object" };
  switch (parsed.type) {
    case "hello": {
      const hello = helloOf(parsed);
      return hello ? { kind: "hello", hello } : { kind: "ignored", reason: "invalid hello" };
    }
    case "frame":
      return frameOf(parsed);
    case "error":
      return {
        kind: "error",
        code: isStr(parsed.code) ? parsed.code : "unknown",
        message: isStr(parsed.message) ? parsed.message : "",
      };
    default:
      return { kind: "ignored", reason: "unknown message type" };
  }
};

export const DECODERS: Readonly<Record<string, Decoder>> = { json: decodeJson };
