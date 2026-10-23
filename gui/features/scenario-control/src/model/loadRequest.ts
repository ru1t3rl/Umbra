import type { ClockSetting, LoadScenarioRequest } from "@umbra/data";
import { parseScale } from "./clockScale.ts";

export type ClockChoice = "" | "realtime" | "afap" | "stepped" | "scaled";

/** The contract pattern of a seed (`^[0-9]{1,20}$`). Kept a string: seeds above 2^53 lose precision as numbers. */
export const SEED_PATTERN = /^[0-9]{1,20}$/;
/** The server also requires the value to fit u64 (`umbra-api-types` seed.rs). */
export const SEED_MAX = "18446744073709551615";

/** A seed the server accepts: 1 to 20 digits that fit u64 (equal-length digit strings compare numerically). */
export function isValidSeed(text: string): boolean {
  if (!SEED_PATTERN.test(text)) return false;
  return text.length < SEED_MAX.length || text <= SEED_MAX;
}

export interface LoadForm {
  path: string;
  clock: ClockChoice;
  scaleText: string;
  seedText: string;
}

export interface LoadFormErrors {
  path: boolean;
  scale: boolean;
  seed: boolean;
}

/** What is wrong with the form. An empty seed is fine (optional); the scale only matters for Scaled. */
export function validateLoadForm(form: LoadForm): LoadFormErrors {
  return {
    path: form.path.trim() === "",
    scale: form.clock === "scaled" && parseScale(form.scaleText) === null,
    seed: form.seedText !== "" && !isValidSeed(form.seedText),
  };
}

/** The request body, or null while the form is invalid. Only what the user set is sent. */
export function buildLoadRequest(form: LoadForm): LoadScenarioRequest | null {
  const errors = validateLoadForm(form);
  if (errors.path || errors.scale || errors.seed) return null;
  let clock: ClockSetting | undefined;
  if (form.clock === "scaled") {
    const scale = parseScale(form.scaleText);
    if (scale === null) return null;
    clock = { mode: "scaled", scale };
  } else if (form.clock !== "") {
    clock = { mode: form.clock };
  }
  return {
    path: form.path.trim(),
    ...(clock ? { clock } : {}),
    ...(form.seedText !== "" ? { seed: form.seedText } : {}),
  };
}
