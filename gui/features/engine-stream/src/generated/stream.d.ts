// GENERATED, DO NOT EDIT. Run `bun run gen:api` (gui/scripts/gen-api.mjs).
/**
 * Any stream message; the root of `api/stream.schema.json`.
 */
export type StreamMessage =
  | (Subscription & {
      type: "subscribe";
    })
  | (
      | (Hello & {
          type: "hello";
        })
      | (Frame & {
          type: "frame";
        })
      | (StreamError & {
          type: "error";
        })
    );

/**
 * Starts or replaces the subscription.
 */
export interface Subscription {
  filter?: StreamFilter | null;
  /**
   * The most frames per second, in `(0, 60]`; default 30.
   */
  max_rate_hz?: number | null;
  /**
   * Whether frames carry `state_hash`; default false.
   */
  state_hash?: boolean | null;
}
/**
 * The filter; default: everything.
 */
export interface StreamFilter {
  bbox?: GeoBox | null;
  /**
   * Only these entities, at most 10,000.
   */
  entity_ids?: number[] | null;
  /**
   * Only events of these kinds.
   */
  event_kinds?: string[] | null;
}
/**
 * Only entities inside the box (and events of them).
 */
export interface GeoBox {
  /**
   * Northern edge.
   */
  max_lat_deg: number;
  /**
   * Eastern edge.
   */
  max_lon_deg: number;
  /**
   * Southern edge.
   */
  min_lat_deg: number;
  /**
   * Western edge.
   */
  min_lon_deg: number;
}
/**
 * The first message.
 */
export interface Hello {
  /**
   * The contract version.
   */
  api_version: string;
  /**
   * The negotiated encoding (`json`).
   */
  encoding: string;
  /**
   * [`STREAM_PROTOCOL`].
   */
  protocol: string;
  settings: StreamSettings;
}
/**
 * The settings in force.
 */
export interface StreamSettings {
  /**
   * The rate used when `subscribe` gives none.
   */
  default_rate_hz: number;
  /**
   * The most `entity_ids` in one subscription.
   */
  max_entity_ids: number;
  /**
   * Events per frame before `events_truncated` is set.
   */
  max_events_per_frame: number;
  /**
   * The longest accepted client message (1009 beyond it).
   */
  max_message_bytes: number;
  /**
   * The highest rate a subscription may ask for.
   */
  max_rate_hz: number;
  /**
   * Frames queued per connection before the oldest is dropped.
   */
  queue_frames: number;
  /**
   * A send that takes longer ends the connection: close code 1008 `too_slow` when the close frame
   * can still be delivered, otherwise the connection is dropped.
   */
  send_timeout_s: number;
}
/**
 * A frame.
 */
export interface Frame {
  /**
   * What caused it.
   */
  cause: "tick" | "control";
  /**
   * Frames the connection never sent since the previous frame.
   */
  dropped: number;
  /**
   * The filtered entities, ascending by id.
   */
  entities: EntitySummary[];
  /**
   * The events since the previous frame, in order.
   */
  events: EventDto[];
  /**
   * Whether events were lost beyond the per-frame cap.
   */
  events_truncated: boolean;
  /**
   * [`FrameKind::Snapshot`].
   */
  kind: "snapshot";
  run: RunStatus;
  /**
   * The number of the frame in its subscription, from 0 at the priming frame. Between two sent
   * frames it grows by `dropped + 1`, so a gap is a drop.
   */
  seq: number;
  /**
   * The session number.
   */
  session: number;
  /**
   * The state hash (16 lower-case hex digits), when the subscription asked for it.
   */
  state_hash?: string | null;
  /**
   * The tick of the snapshot.
   */
  tick: number;
  /**
   * Simulation time in seconds.
   */
  time_s: number;
}
/**
 * An entity as listed by `GET /entities` and carried in stream frames.
 */
export interface EntitySummary {
  /**
   * Altitude above the ellipsoid, metres.
   */
  alt_m: number;
  /**
   * The behavior id, if it has one.
   */
  behavior?: string | null;
  /**
   * Its force.
   */
  force?: string | null;
  /**
   * Its group.
   */
  group?: string | null;
  /**
   * Heading, degrees clockwise from north.
   */
  heading_deg: number;
  /**
   * The entity id; never reused.
   */
  id: number;
  /**
   * Latitude, degrees (WGS-84).
   */
  lat_deg: number;
  /**
   * Longitude, degrees.
   */
  lon_deg: number;
  /**
   * Its unique name, if it has one.
   */
  name?: string | null;
  /**
   * Owned or remote.
   */
  ownership: "owned" | "remote";
  /**
   * The name of the network adapter a remote entity comes from (`name` in
   * `GET /network/adapters`); only when `ownership` is `remote` (added in API 1.1.0).
   */
  remote_source?: string | null;
  route?: RouteProgress | null;
  /**
   * Ground speed, metres per second.
   */
  speed_mps: number;
  /**
   * The behavior's status in the last tick.
   */
  status?: string | null;
  /**
   * Its template.
   */
  template?: string | null;
}
/**
 * Route progress, if it has a route.
 */
export interface RouteProgress {
  /**
   * Number of waypoints.
   */
  len: number;
  /**
   * Index of the next waypoint.
   */
  next: number;
}
/**
 * An event on the stream (best-effort; `dropped` and `events_truncated` make loss visible).
 */
export interface EventDto {
  /**
   * A `custom` event: `{"kind": <component id>, "payload": <the payload>}`. A
   * `simulation_control`: `{"action": "start" | "pause" | "resume" | "stop" | "step"}`, with
   * `"count"` for `step`. A `detonation`: `{"position": {"lat_deg", "lon_deg", "alt_m"}}`, WGS-84
   * like the entities; the shooter and target are `entity_id` and `target_id` when they are
   * mapped to entities of this engine.
   */
  detail?: {} | null;
  /**
   * The main entity: created, removed, shooter (`fire`, `detonation`) or source.
   */
  entity_id?: number | null;
  /**
   * The kind: `entity_created`, `entity_removed`, `fire`, `detonation`, `custom` or
   * `simulation_control` (`detonation` since API 1.1.0). Clients ignore kinds they do not know.
   */
  kind: string;
  /**
   * The target of a `fire` or a `detonation`.
   */
  target_id?: number | null;
  /**
   * The template of an `entity_created`.
   */
  template?: string | null;
  /**
   * The tick it happened in.
   */
  tick: number;
  /**
   * The weapon of a `fire`.
   */
  weapon?: string | null;
}
/**
 * The run status.
 */
export interface RunStatus {
  /**
   * The current clock.
   */
  clock:
    | {
        mode: "afap";
      }
    | {
        mode: "realtime";
      }
    | {
        mode: "stepped";
      }
    | {
        mode: "scaled";
        scale: number;
      };
  fault?: PluginFaultDto | null;
  network_fault?: NetworkFaultDto | null;
  recording?: RecordingDto | null;
  /**
   * The session number; it changes with every load (0 before the first).
   */
  session: number;
  /**
   * The lifecycle state.
   */
  state: "idle" | "ready" | "running" | "paused" | "stopped";
  /**
   * Whether a `step` request is waiting for its ticks.
   */
  step_pending: boolean;
  stop_reason?: ("command" | "duration" | "plugin_fault" | "error" | "network_fault") | null;
  /**
   * The current tick; only when a scenario is loaded. Exact below 2^53.
   */
  tick?: number | null;
  /**
   * The tick rate of the loaded scenario.
   */
  tick_rate_hz?: number | null;
  tick_rate_source?: ("scenario" | "default") | null;
  /**
   * The simulation time in seconds; only when a scenario is loaded.
   */
  time_s?: number | null;
}
/**
 * The plugin fault behind `stop_reason = plugin_fault`.
 */
export interface PluginFaultDto {
  /**
   * The call that faulted.
   */
  call: string;
  /**
   * The faulted capability id.
   */
  capability: string;
  /**
   * The message.
   */
  message: string;
  /**
   * The plugin id.
   */
  plugin: string;
}
/**
 * The network adapter fault behind `stop_reason = network_fault` (added in API 1.1.0).
 */
export interface NetworkFaultDto {
  /**
   * The adapter's name in the scenario.
   */
  adapter: string;
  /**
   * Why it faulted.
   */
  reason: string;
}
/**
 * The session recording; only when the server records (added in API 1.2.0, ADR 0057).
 */
export interface RecordingDto {
  /**
   * The file name inside the record directory, `/` separators, never absolute (ADR 0053, ADR 0058).
   */
  file: string;
  /**
   * The last tick recorded. Exact below 2^53.
   */
  last_tick: number;
  /**
   * Records written so far. Exact below 2^53.
   */
  records: number;
  /**
   * Where the recording stands.
   */
  state: "recording" | "failed" | "finished";
}
/**
 * A problem with a client message.
 */
export interface StreamError {
  /**
   * The stable code.
   */
  code: "invalid_message" | "unknown_message_type" | "invalid_subscription";
  /**
   * A message for a person.
   */
  message: string;
}
