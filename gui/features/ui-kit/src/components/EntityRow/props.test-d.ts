// WI-10 review m9: `badges` is data. `presentational` stays a plain optional prop (not a discriminated union: that would break callers that spread `selected`/`onSelect`, ASM-943).
// Compile-time only (`tsc -b`).
import type { ReactNode } from "react";
import type { EntityRowProps } from "./EntityRow.tsx";

const common = { name: "Alpha", affiliation: "friend" } as const;

export const option: EntityRowProps = { ...common, selected: true, onSelect: () => {} };
export const content: EntityRowProps = { ...common, presentational: true };
export const explicitFalse: EntityRowProps = {
  ...common,
  presentational: false,
  onSelect: () => {},
};
// onSelect and selected are accepted (and ignored) next to `presentational`, so spreading shared props compiles
export const spread: EntityRowProps = { ...option, presentational: true };
export const withBadges: EntityRowProps = {
  ...common,
  badges: [{ label: "Remote", tone: "info" }, { label: "Ghost" }],
};

declare const node: ReactNode;
// @ts-expect-error badges is data (label and optional tone), not a ReactNode
export const nodeBadges: EntityRowProps = { ...common, badges: node };
// @ts-expect-error a badge needs a label
export const noLabel: EntityRowProps = { ...common, badges: [{ tone: "info" }] };
// @ts-expect-error the tone is one of the Badge tones
export const badTone: EntityRowProps = { ...common, badges: [{ label: "x", tone: "loud" }] };
