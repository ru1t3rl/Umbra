import type { EntityFrame } from "@umbra/engine-stream";
import type { CustomPropertySource } from "./colors.ts";

/** Highest zoom the first-frame fit may reach (ASM-307). */
export const FIT_MAX_ZOOM = 15;

/** Per-side padding in px. */
export interface Padding {
  readonly top: number;
  readonly right: number;
  readonly bottom: number;
  readonly left: number;
}

/**
 * Screen space a shell covers with floating UI, in px (the dashboard's bar). A missing, negative or
 * non-finite side is 0. They move the fitted entities and the error banner out from under the overlay.
 */
export interface MapInsets {
  readonly top?: number;
  readonly right?: number;
  readonly bottom?: number;
  readonly left?: number;
}

export interface FitOptions {
  /** Padding in px around the bounds, per side. */
  readonly padding: Padding;
  readonly maxZoom: number;
}

const side = (v: number | undefined): number =>
  v !== undefined && Number.isFinite(v) && v > 0 ? v : 0;

/** The insets with every side a usable number. */
export function normalizeInsets(insets: MapInsets | undefined): Padding {
  return {
    top: side(insets?.top),
    right: side(insets?.right),
    bottom: side(insets?.bottom),
    left: side(insets?.left),
  };
}

/** Fit padding plus the insets, per side: fitted entities never sit under the overlay. */
export function withInsets(fit: FitOptions, insets: MapInsets | undefined): FitOptions {
  const i = normalizeInsets(insets);
  const p = fit.padding;
  return {
    ...fit,
    padding: {
      top: p.top + i.top,
      right: p.right + i.right,
      bottom: p.bottom + i.bottom,
      left: p.left + i.left,
    },
  };
}

/**
 * Where the error banner goes: below the top inset and centred in the area the insets leave free.
 * `calc()` strings so the token `space-4` stays the one source of the gap.
 */
export function bannerPosition(insets: MapInsets | undefined): { top: string; left: string } {
  const i = normalizeInsets(insets);
  return {
    top: `calc(${i.top}px + var(--space-4))`,
    left: `calc(50% + ${(i.left - i.right) / 2}px)`,
  };
}

/** Padding is the `space-16` token (ASM-307); a missing or non-px token throws, like the colours. */
export function fitOptionsFrom(style: CustomPropertySource): FitOptions {
  const raw = style.getPropertyValue("--space-16").trim();
  const match = /^(\d+(?:\.\d+)?)px$/.exec(raw);
  if (!match) throw new Error(`map camera: --space-16 is not a px length: "${raw}"`);
  const px = Number(match[1]);
  return { padding: { top: px, right: px, bottom: px, left: px }, maxZoom: FIT_MAX_ZOOM };
}

/**
 * The camera fits once per session, on its first frame that has entities (an empty first frame waits),
 * and never moves by itself afterwards (ASM-307, ASM-368). `fittedSession` is the last session fitted.
 */
export function needsFit(fittedSession: number | undefined, frame: EntityFrame): boolean {
  return frame.entities.length > 0 && frame.session !== fittedSession;
}
