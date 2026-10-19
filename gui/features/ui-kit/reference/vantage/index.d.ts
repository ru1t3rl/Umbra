import type * as React from 'react';

export type Affiliation = 'friend' | 'hostile' | 'neutral' | 'unknown';
export type Tone = 'neutral' | 'success' | 'warning' | 'danger' | 'info';

/** Material Symbols Rounded glyph, by ligature name ("play_arrow"). */
export interface IconProps { name: string }
export declare function Icon(props: IconProps): React.ReactElement;

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  /** filled = the one azure command per view; tonal is the default; outline and ghost for secondary; danger for destructive. */
  variant?: 'filled' | 'tonal' | 'outline' | 'ghost' | 'danger';
  size?: 'md' | 'sm';
  /** Material Symbols name. Without children the button is icon-only: provide aria-label. */
  icon?: string;
}
export declare function Button(props: ButtonProps): React.ReactElement;

export interface ChipProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  selected?: boolean;
  /** Glass fill for chips floating over the map. */
  glass?: boolean;
  icon?: string;
}
export declare function Chip(props: ChipProps): React.ReactElement;
export interface ChipGroupProps { label: string; className?: string; children?: React.ReactNode }
export declare function ChipGroup(props: ChipGroupProps): React.ReactElement;

export interface PanelProps {
  title: string;
  /** Header-right slot: icon Buttons (size sm, ghost). */
  actions?: React.ReactNode;
  /** 320px by default; 400px for tables. */
  width?: number | string;
  className?: string;
  children?: React.ReactNode;
}
export declare function Panel(props: PanelProps): React.ReactElement;

export interface CardProps {
  title?: string;
  subtitle?: string;
  /** Tonal level: 1 = surface-1, 2 = surface-2 (default), 3 = surface-3. */
  level?: 1 | 2 | 3;
  variant?: 'tonal' | 'outline';
  className?: string;
  children?: React.ReactNode;
}
export declare function Card(props: CardProps): React.ReactElement;

export interface TextFieldProps extends React.InputHTMLAttributes<HTMLInputElement> {
  label: string;
  help?: string;
  /** Mono input for coordinates, IDs and times. */
  mono?: boolean;
  invalid?: boolean;
}
export declare function TextField(props: TextFieldProps): React.ReactElement;

export interface ToggleProps { checked: boolean; onChange?: (next: boolean) => void; label?: string; disabled?: boolean; className?: string }
export declare function Toggle(props: ToggleProps): React.ReactElement;

export interface SliderProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, 'type'> {
  label: string;
  value: number;
  /** Formats the live value shown in cyan ("1:4", "60 Hz"). */
  format?: (value: number) => string;
}
export declare function Slider(props: SliderProps): React.ReactElement;

export interface TabItem { value: string; label: string; count?: number }
export interface TabsProps { items: TabItem[]; value: string; onChange?: (value: string) => void; className?: string }
export declare function Tabs(props: TabsProps): React.ReactElement;

export interface CommandBarProps {
  brand?: string;
  title: string;
  subtitle?: string;
  /** Scenario clock text, e.g. "14:32:07Z". Cyan while running. */
  clock?: string;
  paused?: boolean;
  /** Glass, radius-xl, floats over the map. */
  floating?: boolean;
  className?: string;
  /** Right-hand actions: Chips, Buttons, Toggle. */
  children?: React.ReactNode;
}
export declare function CommandBar(props: CommandBarProps): React.ReactElement;

export interface ReadoutProps {
  label: string;
  value: React.ReactNode;
  unit?: string;
  /** Cyan value: changes at runtime. */
  live?: boolean;
  delta?: string;
  trend?: 'up' | 'down' | 'flat';
  className?: string;
}
export declare function Readout(props: ReadoutProps): React.ReactElement;

export interface BadgeProps { tone?: Tone; className?: string; children?: React.ReactNode }
export declare function Badge(props: BadgeProps): React.ReactElement;

export interface AffiliationTagProps {
  affiliation: Affiliation;
  /** Replaces the default word with an echelon or short code ("BN", "2 MECH"). */
  children?: React.ReactNode;
  className?: string;
}
export declare function AffiliationTag(props: AffiliationTagProps): React.ReactElement;

export interface EntityRowProps {
  name: string;
  /** DIS/HLA identifier shown in mono beside the name. */
  id?: string;
  affiliation: Affiliation;
  echelon?: string;
  /** One mono line: position, heading, speed. */
  meta?: string;
  status?: { label: string; tone: Tone };
  selected?: boolean;
  stale?: boolean;
  onSelect?: () => void;
  className?: string;
}
export declare function EntityRow(props: EntityRowProps): React.ReactElement;
export interface EntityListProps { label: string; className?: string; children?: React.ReactNode }
export declare function EntityList(props: EntityListProps): React.ReactElement;

export interface AlertProps {
  tone?: 'info' | 'success' | 'warning' | 'danger';
  title: string;
  /** A ghost or tonal Button, size sm. */
  action?: React.ReactNode;
  className?: string;
  children?: React.ReactNode;
}
export declare function Alert(props: AlertProps): React.ReactElement;

declare global {
  interface Window {
    Vantage: {
      Icon: typeof Icon; Button: typeof Button; Chip: typeof Chip; ChipGroup: typeof ChipGroup; Panel: typeof Panel; Card: typeof Card;
      TextField: typeof TextField; Toggle: typeof Toggle; Slider: typeof Slider; Tabs: typeof Tabs; CommandBar: typeof CommandBar;
      Readout: typeof Readout; Badge: typeof Badge; AffiliationTag: typeof AffiliationTag; EntityRow: typeof EntityRow;
      EntityList: typeof EntityList; Alert: typeof Alert;
    };
  }
}
