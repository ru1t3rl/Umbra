import { t } from "../messages.ts";

// The clock scale: slider (logarithmic) and numeric field (the full contract range), ASM-811.

/** Contract range of `ClockSetting` scaled.scale. */
export const SCALE_RANGE = { min: 0.001, max: 1_000_000 } as const;
/** 100 positions per decade over 0.1x..100x. */
export const SLIDER = { min: 0, max: 300, step: 1 } as const;

/** 10^(pos/100 - 1), rounded to 3 significant digits so the field shows "3.16", not float noise. */
export function scaleFromPosition(position: number): number {
  return Number((10 ** (position / 100 - 1)).toPrecision(3));
}

/** The slider position of a scale, rounded and clamped (a scale beyond 0.1x..100x parks the thumb at an end). */
export function positionFromScale(scale: number): number {
  const position = Math.round(100 * (Math.log10(scale) + 1));
  return Math.min(SLIDER.max, Math.max(SLIDER.min, position));
}

const PLAIN_DECIMAL = /^(?:\d+\.?\d*|\.\d+)$/;

/** A plain decimal within the contract range, else null (no exponent, sign, separators or words). */
export function parseScale(text: string): number | null {
  const trimmed = text.trim();
  if (!PLAIN_DECIMAL.test(trimmed)) return null;
  const value = Number(trimmed);
  return value >= SCALE_RANGE.min && value <= SCALE_RANGE.max ? value : null;
}

export function formatScale(scale: number): string {
  return t("clock.factor", { scale });
}
