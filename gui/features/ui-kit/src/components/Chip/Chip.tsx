import type { ButtonHTMLAttributes, ReactElement, ReactNode } from "react";
import { Icon } from "../Icon/Icon.tsx";
import "./Chip.css";

export interface ChipProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  selected?: boolean;
  /** Glass fill for chips floating over the map. */
  glass?: boolean;
  icon?: string;
}

export function Chip({
  selected = false,
  glass,
  icon,
  children,
  className,
  ...rest
}: ChipProps): ReactElement {
  const classes = ["vt-chip", glass && "vt-chip-glass", className].filter(Boolean).join(" ");
  return (
    <button type="button" aria-pressed={selected} {...rest} className={classes}>
      {icon ? <Icon name={icon} /> : null}
      {children}
    </button>
  );
}

export interface ChipGroupProps {
  label: string;
  className?: string;
  children?: ReactNode;
}

export function ChipGroup({ label, className, children }: ChipGroupProps): ReactElement {
  const classes = ["vt-chipgroup", className].filter(Boolean).join(" ");
  return (
    // biome-ignore lint/a11y/useSemanticElements: the reference is a div with role=group; a fieldset would need a legend and UA styling resets (ASM-335)
    <div className={classes} role="group" aria-label={label}>
      {children}
    </div>
  );
}
