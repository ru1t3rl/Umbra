import { type InputHTMLAttributes, type ReactElement, useId } from "react";
import "../shared/Label.css";
import "./TextField.css";

export interface TextFieldProps extends InputHTMLAttributes<HTMLInputElement> {
  label: string;
  help?: string;
  /** Mono input for coordinates, IDs and times. */
  mono?: boolean;
  invalid?: boolean;
}

export function TextField({
  label,
  help,
  mono,
  invalid,
  className,
  id,
  "aria-describedby": describedBy,
  ...rest
}: TextFieldProps): ReactElement {
  const generated = useId();
  const inputId = id || generated;
  const helpId = `${inputId}-help`;
  const describedByIds = [describedBy, help ? helpId : undefined].filter(Boolean).join(" ");
  const classes = ["vt-field", mono && "vt-field-mono", invalid && "vt-field-invalid", className]
    .filter(Boolean)
    .join(" ");
  return (
    <div className={classes}>
      <label className="vt-label" htmlFor={inputId}>
        {label}
      </label>
      <input
        {...rest}
        id={inputId}
        aria-invalid={invalid ? true : rest["aria-invalid"]}
        aria-describedby={describedByIds || undefined}
      />
      {help ? (
        <span className="vt-help" id={helpId}>
          {help}
        </span>
      ) : null}
    </div>
  );
}
