import type { EntityFrame, EntitySummary } from "@umbra/engine-stream";
import type { MapColors } from "./colors.ts";

/** Sentinel colours: obviously not real tokens, so a test can tell a passed-in value from a literal. */
export const COLORS: MapColors = {
  surface0: "S0",
  surface1: "S1",
  line: "LINE",
  lineStrong: "LINE-STRONG",
  inkMuted: "MUTED",
  symbolFrame: "FRAME",
  affiliation: {
    friend: "A-FRIEND",
    hostile: "A-HOSTILE",
    neutral: "A-NEUTRAL",
    unknown: "A-UNKNOWN",
  },
  selection: "S-AZURE",
  opacityTrack: 0.55,
};

export const entity = (id: number, extra: Partial<EntitySummary> = {}): EntitySummary => ({
  id,
  alt_m: 0,
  heading_deg: 90,
  lat_deg: 10 + id * 0.001,
  lon_deg: 20 + id * 0.001,
  ownership: "owned",
  speed_mps: 5,
  force: "blue",
  ...extra,
});

export const frameOf = (entities: readonly EntitySummary[]): EntityFrame => ({
  session: 1,
  tick: 1,
  time_s: 0,
  seq: 0,
  dropped: 0,
  entities,
});
