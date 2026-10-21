// Type-level tests, checked by `tsc -b` (bun test does not run this file). Each @ts-expect-error
// line must keep failing to compile; if the prop contract loosens, tsc reports "unused directive".
import type { ButtonProps } from "./Button/Button.tsx";

export const ok: ButtonProps[] = [
  { children: "Start" },
  { icon: "play_arrow", children: "Start" },
  { icon: "play_arrow", "aria-label": "Start" },
  { icon: "play_arrow", "aria-labelledby": "label-id" },
  { icon: "play_arrow", "aria-labelledby": "x", children: "Start", variant: "filled", size: "sm" },
];

// @ts-expect-error icon-only Button requires aria-label
export const iconOnlyNoLabel: ButtonProps = { icon: "play_arrow" };

// @ts-expect-error a Button needs children or an icon with aria-label
export const empty: ButtonProps = {};

// @ts-expect-error unknown variant
export const badVariant: ButtonProps = { children: "x", variant: "cyan" };

// @ts-expect-error undefined children do not count as content
export const undefinedChildren: ButtonProps = { icon: "play_arrow", children: undefined };

// @ts-expect-error null children do not count as content
export const nullChildren: ButtonProps = { icon: "play_arrow", children: null };
