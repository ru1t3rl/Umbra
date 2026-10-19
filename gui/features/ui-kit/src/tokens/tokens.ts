// GENERATED from tokens.json by gui/scripts/gen-tokens.mjs. Do not edit.
export const colorTokens = ["surface-0","surface-1","surface-2","surface-3","glass","glass-stroke","line","line-strong","ink","ink-muted","ink-faint","azure","on-azure","azure-dim","cyan","on-cyan","cyan-dim","success","success-dim","danger","danger-dim","info","warning","warning-dim","on-warning","state-hover","state-pressed","focus-ring","scrim","affil-friend","affil-hostile","affil-neutral","affil-unknown","symbol-frame"] as const;
export const spacingTokens = ["space-1","space-2","space-3","space-4","space-5","space-6","space-8","space-10","space-12","space-16"] as const;
export const radiusTokens = ["radius-xs","radius-sm","radius-md","radius-lg","radius-xl","radius-full"] as const;
export const typeStyles = ["display-lg","headline","title","title-sm","body-lg","body","body-strong","label","caption","readout","data-lg","data","data-sm"] as const;
export type ColorToken = (typeof colorTokens)[number];
export type SpacingToken = (typeof spacingTokens)[number];
export type RadiusToken = (typeof radiusTokens)[number];
export type TypeStyle = (typeof typeStyles)[number];
export type Affiliation = "friend" | "hostile" | "neutral" | "unknown";
/** CSS custom property reference for a token, e.g. cssVar("azure") -> "var(--azure)". */
export const cssVar = (name: ColorToken | SpacingToken | RadiusToken): string => `var(--${name})`;
