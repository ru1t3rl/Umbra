import type { ButtonHTMLAttributes, ReactElement } from "react";
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
