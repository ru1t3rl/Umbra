import type { ReactElement } from "react";
import "./Icon.css";

export interface IconProps {
  /** Material Symbols ligature name, e.g. "layers". Decorative: hidden from assistive technology. */
  name: string;
}

export function Icon({ name }: IconProps): ReactElement {
  return (
    <span className="vt-icon" aria-hidden="true">
      {name}
    </span>
  );
}
