import { t } from "../messages.ts";

const pad = (n: number): string => String(n).padStart(2, "0");

/**
 * Elapsed simulation time as `T+hh:mm:ss` (ASM-308). Seconds are floored, so the display never runs ahead
 * of the engine. Hours do not wrap: 100 hours is `T+100:00:00`. A missing, negative or non-finite value
 * (no scenario loaded, a bad frame) is the placeholder, never a made-up time.
 */
export function formatSimTime(time_s: number | null | undefined): string {
  if (time_s === null || time_s === undefined || !Number.isFinite(time_s) || time_s < 0) {
    return t("readout.none");
  }
  const total = Math.floor(time_s);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  return `T+${pad(hours)}:${pad(minutes)}:${pad(total % 60)}`;
}
