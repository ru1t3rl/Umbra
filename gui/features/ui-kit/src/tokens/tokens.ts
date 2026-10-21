// GENERATED from tokens.json by gui/scripts/gen-tokens.mjs. Do not edit.
export const colorTokens = ["surface-0","surface-1","surface-2","surface-3","glass","glass-stroke","line","line-strong","ink","ink-muted","ink-faint","azure","on-azure","azure-dim","cyan","on-cyan","cyan-dim","success","success-dim","danger","danger-dim","info","warning","warning-dim","on-warning","state-hover","state-pressed","focus-ring","scrim","affil-friend","affil-hostile","affil-neutral","affil-unknown","symbol-frame"] as const;
export const spacingTokens = ["space-1","space-2","space-3","space-4","space-5","space-6","space-8","space-10","space-12","space-16"] as const;
export const radiusTokens = ["radius-xs","radius-sm","radius-md","radius-lg","radius-xl","radius-full"] as const;
export const shadowTokens = ["shadow-glass","shadow-menu","shadow-focus"] as const;
export const blurTokens = ["blur-sm","blur-md","blur-lg"] as const;
export const opacityTokens = ["opacity-disabled","opacity-disabled-fill","opacity-track"] as const;
export const durationTokens = ["duration-fast","duration-base","duration-slow"] as const;
export const easingTokens = ["ease-standard","ease-exit"] as const;
export const typeStyles = ["display-lg","headline","title","title-sm","body-lg","body","body-strong","label","caption","readout","data-lg","data","data-sm"] as const;
export type ColorToken = (typeof colorTokens)[number];
export type SpacingToken = (typeof spacingTokens)[number];
export type RadiusToken = (typeof radiusTokens)[number];
export type ShadowToken = (typeof shadowTokens)[number];
export type BlurToken = (typeof blurTokens)[number];
export type OpacityToken = (typeof opacityTokens)[number];
export type DurationToken = (typeof durationTokens)[number];
export type EasingToken = (typeof easingTokens)[number];
export type TypeStyle = (typeof typeStyles)[number];
export type Affiliation = "friend" | "hostile" | "neutral" | "unknown";
/** CSS custom property reference for a token, e.g. cssVar("azure") -> "var(--azure)". */
export type CssVarToken =
  | ColorToken
  | SpacingToken
  | RadiusToken
  | ShadowToken
  | BlurToken
  | OpacityToken
  | DurationToken
  | EasingToken;
export const cssVar = (name: CssVarToken): string => `var(--${name})`;
