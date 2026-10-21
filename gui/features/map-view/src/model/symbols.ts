import type { Affiliation } from "@umbra/ui-kit";
import type { MapColors } from "./colors.ts";

/**
 * APP-6D affiliation frames drawn from the resolved token colours (ADR 0042). No colour literal lives
 * here: every fill and stroke is a value passed in `colors` (the tests prove it), or `none`.
 */

/** Ownership as the stream reports it (ASM-359). Only `owned` is drawn without the ring (ASM-362). */
export type SymbolOwnership = "owned" | "remote" | "unknown";

/** Frame box in px: the nominal symbol size at the default zoom. */
export const SYMBOL_FRAME_PX = 24;
/** Frame and direction-line stroke width in px (tactical-display.md). */
export const SYMBOL_STROKE_PX = 1.5;
/** Ring stroke width in px (tactical-display.md, remote-entity rule). */
export const RING_STROKE_PX = 1;
/** Gap in px between the outside of the frame stroke and the inside of the ring stroke. */
export const RING_GAP_PX = 3;
/** Ring radius (to the stroke centre): farthest frame vertex (the 24x16 rectangle's corner, 14.42) + 0.75 + 3 + 0.5, rounded up. */
export const RING_RADIUS_PX = 19;
/** Image canvas in px: the ring (radius 19, outer edge 19.5) fits in 40, the frame is centred. */
export const SYMBOL_CANVAS_PX = 40;
/** Direction image canvas in px: centred on the entity so MapLibre can rotate it about the symbol. */
export const DIRECTION_CANVAS_PX = 72;

const C = SYMBOL_CANVAS_PX / 2;

/** SVG shape element (without paint attributes) for each affiliation. */
const FRAMES: Readonly<Record<Affiliation, string>> = {
  // friend: rectangle (3:2)
  friend: `<rect x="${C - 12}" y="${C - 8}" width="24" height="16"`,
  // hostile: diamond
  hostile: `<polygon points="${C},${C - 12} ${C + 12},${C} ${C},${C + 12} ${C - 12},${C}"`,
  // neutral: square, about 0.73x the friend width (APP-6D proportions)
  neutral: `<rect x="${C - 9}" y="${C - 9}" width="18" height="18"`,
  // unknown: quatrefoil (four lobes)
  unknown: `<path d="M${C - 6} ${C - 6}C${C - 6} ${C - 12} ${C + 6} ${C - 12} ${C + 6} ${C - 6}C${C + 12} ${C - 6} ${C + 12} ${C + 6} ${C + 6} ${C + 6}C${C + 6} ${C + 12} ${C - 6} ${C + 12} ${C - 6} ${C + 6}C${C - 12} ${C + 6} ${C - 12} ${C - 6} ${C - 6} ${C - 6}Z"`,
};

const svg = (size: number, body: string): string =>
  `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 ${size} ${size}">${body}</svg>`;

/** The id under which the map registers the image (`addImage`). Only `owned` has the plain image. */
export function symbolImageId(affiliation: Affiliation, ownership: SymbolOwnership): string {
  return `symbol-${affiliation}-${ownership === "owned" ? "plain" : "ring"}`;
}

export const DIRECTION_IMAGE_ID = "direction";

/**
 * Affiliation fill, `symbol-frame` stroke. A 1px `ink-muted` outer ring for every ownership except
 * `owned`: the ring is keyed on the control-gating predicate, so "remote or unknown" is marked.
 */
export function symbolSvg(
  affiliation: Affiliation,
  ownership: SymbolOwnership,
  colors: MapColors,
): string {
  const frame = `${FRAMES[affiliation]} fill="${colors.affiliation[affiliation]}" stroke="${colors.symbolFrame}" stroke-width="${SYMBOL_STROKE_PX}"/>`;
  const ring =
    ownership === "owned"
      ? ""
      : `<circle cx="${C}" cy="${C}" r="${RING_RADIUS_PX}" fill="none" stroke="${colors.inkMuted}" stroke-width="${RING_STROKE_PX}"/>`;
  return svg(SYMBOL_CANVAS_PX, ring + frame);
}

/**
 * APP-6D direction-of-movement line pointing north from the centre, 1.5x the symbol long (ADR 0042),
 * (36,36) to (36,0). The canvas is centred on the entity, so rotating the image by the heading rotates it
 * about the symbol. The `direction` layer sits below the symbols, so the frame covers the inner part.
 */
export function directionSvg(colors: MapColors): string {
  const mid = DIRECTION_CANVAS_PX / 2;
  return svg(
    DIRECTION_CANVAS_PX,
    `<line x1="${mid}" y1="${mid}" x2="${mid}" y2="${mid - SYMBOL_FRAME_PX * 1.5}" fill="none" stroke="${colors.symbolFrame}" stroke-width="${SYMBOL_STROKE_PX}"/>`,
  );
}

export const AFFILIATIONS: readonly Affiliation[] = ["friend", "hostile", "neutral", "unknown"];

export interface SymbolImage {
  readonly id: string;
  readonly svg: string;
  readonly size: number;
}

/** Every image the style needs: 4 affiliations x {plain, ring}, plus the direction line (9 images). */
export function symbolImages(colors: MapColors): readonly SymbolImage[] {
  const out: SymbolImage[] = [];
  for (const a of AFFILIATIONS) {
    for (const o of ["owned", "remote"] as const) {
      out.push({ id: symbolImageId(a, o), svg: symbolSvg(a, o, colors), size: SYMBOL_CANVAS_PX });
    }
  }
  out.push({ id: DIRECTION_IMAGE_ID, svg: directionSvg(colors), size: DIRECTION_CANVAS_PX });
  return out;
}
