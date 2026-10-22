// The step count of `StepRequest` (1..=1000000) and the preset buttons (ASM-810).

export const STEP_MIN = 1;
export const STEP_MAX = 1_000_000;
export const STEP_PRESETS: readonly number[] = [1, 10, 100, 1000];

const DIGITS = /^[0-9]+$/;

/** ASCII digits only (after trimming), an integer in the contract range; else null. */
export function parseStepCount(text: string): number | null {
  const trimmed = text.trim();
  if (!DIGITS.test(trimmed)) return null;
  const value = Number(trimmed);
  return value >= STEP_MIN && value <= STEP_MAX ? value : null;
}
