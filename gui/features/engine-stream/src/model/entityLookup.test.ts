import { expect, test } from "bun:test";
import type { EntitySummary } from "../types.ts";
import { sameEntity } from "./entityLookup.ts";

const base: EntitySummary = {
  id: 1,
  lat_deg: 1,
  lon_deg: 2,
  alt_m: 3,
  heading_deg: 4,
  speed_mps: 5,
  ownership: "remote",
};

test("remote_source counts as a change", () => {
  // Mutation caught: remote_source missing from the compared fields.
  expect(sameEntity({ ...base, remote_source: "a" }, { ...base, remote_source: "b" })).toBe(false);
  expect(sameEntity({ ...base, remote_source: "a" }, { ...base, remote_source: "a" })).toBe(true);
});
