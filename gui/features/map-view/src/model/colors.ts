import type { Affiliation } from "@umbra/ui-kit";

/** The token values the map needs, resolved to strings MapLibre accepts (it cannot read `var(--x)`). */
export interface MapColors {
  readonly surface0: string;
  readonly surface1: string;
  readonly line: string;
  readonly lineStrong: string;
  readonly inkMuted: string;
  readonly symbolFrame: string;
  readonly affiliation: Readonly<Record<Affiliation, string>>;
  /** `--opacity-track` as a number in 0..1. */
  readonly opacityTrack: number;
}

/** The part of `CSSStyleDeclaration` this needs, so the model stays DOM-free. */
export interface CustomPropertySource {
  getPropertyValue(name: string): string;
}

function read(style: CustomPropertySource, token: string): string {
  const value = style.getPropertyValue(`--${token}`).trim();
  if (value === "") throw new Error(`map colours: the token --${token} is not defined`);
  return value;
}

/**
 * Reads the token custom properties. `LiveMap` passes `getComputedStyle(document.documentElement)`, so the
 * active theme applies. Throws if a token is missing or `--opacity-track` is not a number in 0..1.
 */
export function resolveMapColors(style: CustomPropertySource): MapColors {
  const opacityTrack = Number(read(style, "opacity-track"));
  if (!(opacityTrack >= 0 && opacityTrack <= 1)) {
    throw new Error("map colours: --opacity-track is not a number between 0 and 1");
  }
  return {
    surface0: read(style, "surface-0"),
    surface1: read(style, "surface-1"),
    line: read(style, "line"),
    lineStrong: read(style, "line-strong"),
    inkMuted: read(style, "ink-muted"),
    symbolFrame: read(style, "symbol-frame"),
    affiliation: {
      friend: read(style, "affil-friend"),
      hostile: read(style, "affil-hostile"),
      neutral: read(style, "affil-neutral"),
      unknown: read(style, "affil-unknown"),
    },
    opacityTrack,
  };
}
