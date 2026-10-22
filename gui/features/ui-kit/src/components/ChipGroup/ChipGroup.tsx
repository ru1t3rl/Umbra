import type { ReactElement, ReactNode } from "react";
import "./ChipGroup.css";

export interface ChipGroupProps {
  label: string;
  className?: string;
  /** Id of the element that describes the group (help text). */
  "aria-describedby"?: string;
  children?: ReactNode;
}

export function ChipGroup({
  label,
  className,
  children,
  "aria-describedby": describedBy,
}: ChipGroupProps): ReactElement {
  const classes = ["vt-chipgroup", className].filter(Boolean).join(" ");
  return (
    // biome-ignore lint/a11y/useSemanticElements: the reference is a div with role=group; a fieldset would need a legend and UA styling resets (ASM-335)
    <div className={classes} role="group" aria-label={label} aria-describedby={describedBy}>
      {children}
    </div>
  );
}
