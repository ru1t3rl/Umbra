// Type-level tests, checked by `tsc -b` (bun test does not run this file). Each @ts-expect-error
// line must keep failing to compile; if the prop contract loosens, tsc reports "unused directive".
import type { AppLinkProps } from "./AppLink/AppLink.tsx";
import type { ButtonProps } from "./Button/Button.tsx";
import type { CommandBarProps } from "./CommandBar/CommandBar.tsx";
import type { EntityRowProps } from "./EntityRow/EntityRow.tsx";
import type { NavItem, NavigationProviderProps } from "./NavigationProvider/NavigationProvider.tsx";
import type { PanelProps } from "./Panel/Panel.tsx";
import type { ReadoutProps } from "./Readout/Readout.tsx";
import type { SelectProps } from "./Select/Select.tsx";
import type { SliderProps } from "./Slider/Slider.tsx";
import type { TableProps } from "./Table/Table.tsx";
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

// CommandBar: paused needs a clock.
export const barOk: CommandBarProps[] = [
  { title: "T" },
  { title: "T", clock: "14:32:07Z" },
  { title: "T", clock: "14:32:07Z", paused: true, floating: true },
];

// @ts-expect-error paused without a clock has nothing to pause
export const barPausedNoClock: CommandBarProps = { title: "T", paused: true };

// @ts-expect-error title is required
export const barNoTitle: CommandBarProps = {};

// Readout: a trend describes a delta.
export const readoutOk: ReadoutProps[] = [
  { label: "A", value: 1 },
  { label: "A", value: "1", unit: "Hz", live: true },
  { label: "A", value: 1, delta: "+3", trend: "up" },
  { label: "A", value: 1, delta: "+3" },
];

// @ts-expect-error a trend without a delta shows nothing
export const readoutTrendNoDelta: ReadoutProps = { label: "A", value: 1, trend: "up" };

export const readoutBadTrend: ReadoutProps = {
  label: "A",
  value: 1,
  delta: "x",
  // @ts-expect-error unknown trend
  trend: "sideways",
};

// @ts-expect-error EntityRow needs name and affiliation
export const rowMissing: EntityRowProps = { id: "1" };

// @ts-expect-error unknown affiliation
export const rowBadAffiliation: EntityRowProps = { name: "A", affiliation: "ally" };

// Table: caption, columns, rows and rowKey are required; align is start or end.
interface TableRow {
  id: string;
}
export const tableOk: TableProps<TableRow>[] = [
  { caption: "C", columns: [{ key: "id", label: "Id" }], rows: [], rowKey: (r) => r.id },
  {
    caption: "C",
    columns: [{ key: "id", label: "Id", mono: true, align: "end" }],
    rows: [{ id: "1" }],
    rowKey: (r) => r.id,
    empty: "None",
    dense: true,
    captionHidden: true,
  },
];

// @ts-expect-error a Table needs a caption to be named
export const tableNoCaption: TableProps<TableRow> = { columns: [], rows: [], rowKey: (r) => r.id };

// @ts-expect-error a Table needs rowKey
export const tableNoRowKey: TableProps<TableRow> = { caption: "C", columns: [], rows: [] };

export const tableBadAlign: TableProps<TableRow> = {
  caption: "C",
  // @ts-expect-error unknown align
  columns: [{ key: "id", label: "Id", align: "middle" }],
  rows: [],
  rowKey: (r) => r.id,
};

export const tableUnknownColumnKey: TableProps<TableRow> = {
  caption: "C",
  // @ts-expect-error a column key must be a key of the row type
  columns: [{ key: "nope", label: "Id" }],
  rows: [],
  rowKey: (r) => r.id,
};

// Select: label, options, value and onChange(value: string).
export const selectOk: SelectProps[] = [
  { label: "L", options: [{ value: "a", label: "A" }], value: "a", onChange: (_v: string) => {} },
  {
    label: "L",
    options: [{ value: "a", label: "A", disabled: true }],
    value: "a",
    onChange: () => {},
    help: "h",
    invalid: true,
    disabled: true,
  },
];

// @ts-expect-error Select requires a label
export const selectNoLabel: SelectProps = { options: [], value: "", onChange: () => {} };

// @ts-expect-error Select requires options
export const selectNoOptions: SelectProps = { label: "L", value: "", onChange: () => {} };

export const selectNumberValue: SelectProps = {
  label: "L",
  options: [],
  // @ts-expect-error Select value is a string
  value: 1,
  onChange: () => {},
};

export const selectOptionNoLabel: SelectProps = {
  label: "L",
  // @ts-expect-error an option needs a label
  options: [{ value: "a" }],
  value: "a",
  onChange: () => {},
};

// NavItem and NavigationProvider.
export const navItemOk: NavItem[] = [
  { path: "/", label: "Map" },
  { path: "/p", label: "P", icon: "map" },
];

// @ts-expect-error a NavItem needs a label
export const navItemNoLabel: NavItem = { path: "/" };

// @ts-expect-error a NavItem needs a path
export const navItemNoPath: NavItem = { label: "Map" };

// @ts-expect-error NavigationProvider requires navigate
export const providerNoNavigate: NavigationProviderProps = {
  items: [],
  currentPath: "/",
  children: null,
};

// @ts-expect-error NavigationProvider requires currentPath
export const providerNoPath: NavigationProviderProps = {
  items: [],
  navigate: () => {},
  children: null,
};

// AppLink: `to` is required.
// @ts-expect-error AppLink needs a target
export const linkNoTo: AppLinkProps = { children: "x" };
export const linkOk: AppLinkProps = { to: "/x", children: "x" };
