import type { EntityFrame } from "@umbra/engine-stream";
import type { CustomPropertySource } from "./colors.ts";

/** Highest zoom the first-frame fit may reach (ASM-307). */
export const FIT_MAX_ZOOM = 15;

export interface FitOptions {
  /** Padding in px around the bounds. */
  readonly padding: number;
  readonly maxZoom: number;
}

/** Padding is the `space-16` token (ASM-307); a missing or non-px token throws, like the colours. */
export function fitOptionsFrom(style: CustomPropertySource): FitOptions {
  const raw = style.getPropertyValue("--space-16").trim();
  const match = /^(\d+(?:\.\d+)?)px$/.exec(raw);
  if (!match) throw new Error(`map camera: --space-16 is not a px length: "${raw}"`);
  return { padding: Number(match[1]), maxZoom: FIT_MAX_ZOOM };
}

/**
 * The camera fits once per session, on its first frame that has entities (an empty first frame waits),
 * and never moves by itself afterwards (ASM-307, ASM-368). `fittedSession` is the last session fitted.
 */
export function needsFit(fittedSession: number | undefined, frame: EntityFrame): boolean {
  return frame.entities.length > 0 && frame.session !== fittedSession;
}
