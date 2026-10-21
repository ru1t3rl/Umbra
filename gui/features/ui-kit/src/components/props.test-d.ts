// Type-level tests, checked by `tsc -b` (bun test does not run this file). Each @ts-expect-error
// line must keep failing to compile; if the prop contract loosens, tsc reports "unused directive".
import type { ButtonProps } from "./Button/Button.tsx";
import type { PanelProps } from "./Panel/Panel.tsx";
import type { SliderProps } from "./Slider/Slider.tsx";
import type { TextFieldProps } from "./TextField/TextField.tsx";
import type { ToggleProps } from "./Toggle/Toggle.tsx";

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

// Toggle: a visible label, or an accessible name another way.
export const toggleOk: ToggleProps[] = [
  { checked: true, label: "Record session" },
  { checked: true, "aria-label": "Day theme" },
  { checked: true, "aria-labelledby": "id" },
];

// @ts-expect-error a Toggle without label, aria-label or aria-labelledby has no name
export const toggleUnnamed: ToggleProps = { checked: true };

// @ts-expect-error checked is required
export const toggleNoChecked: ToggleProps = { label: "x" };

// @ts-expect-error TextField requires a label
export const fieldNoLabel: TextFieldProps = { help: "x" };

// @ts-expect-error Slider value is a number
export const sliderStringValue: SliderProps = { label: "Rate", value: "4" };

// @ts-expect-error Slider type is fixed to range
export const sliderType: SliderProps = { label: "Rate", value: 4, type: "text" };

// @ts-expect-error Panel requires a title
export const panelNoTitle: PanelProps = {};
