import { type ReactElement, useEffect } from "react";
import "./Toggle.css";

interface ToggleBaseProps {
  checked: boolean;
  /** Called with the requested next value; the parent owns the state. */
  onChange?: (next: boolean) => void;
  disabled?: boolean;
  className?: string;
  /** Id of the element that describes the switch (help text). */
  "aria-describedby"?: string;
}

/** Visible label: it names the switch. */
interface ToggleLabelledProps extends ToggleBaseProps {
  label: string;
  "aria-label"?: string;
  "aria-labelledby"?: string;
}

/** No visible label: the switch must be named another way. */
type ToggleUnlabelledProps = ToggleBaseProps & {
  label?: undefined;
} & (
    | { "aria-label": string; "aria-labelledby"?: string }
    | { "aria-labelledby": string; "aria-label"?: string }
  );

export type ToggleProps = ToggleLabelledProps | ToggleUnlabelledProps;

export function Toggle({
  checked,
  onChange,
  label,
  disabled,
  className,
  "aria-label": ariaLabel,
  "aria-labelledby": ariaLabelledBy,
  "aria-describedby": ariaDescribedBy,
}: ToggleProps): ReactElement {
  const unnamed = !label && !ariaLabel && !ariaLabelledBy;
  useEffect(() => {
    if (unnamed && process.env.NODE_ENV !== "production") {
      console.warn("@umbra/ui-kit Toggle: a switch needs a label, aria-label or aria-labelledby.");
    }
  }, [unnamed]);
  const classes = ["vt-toggle", className].filter(Boolean).join(" ");
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={ariaLabel}
      aria-labelledby={ariaLabelledBy}
      aria-describedby={ariaDescribedBy}
      className={classes}
      disabled={disabled}
      onClick={() => onChange?.(!checked)}
    >
      <span className="vt-toggle-track">
        <span className="vt-toggle-thumb" />
      </span>
      {label ? <span>{label}</span> : null}
    </button>
  );
}
