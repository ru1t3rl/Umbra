import { type ReactElement, type SelectHTMLAttributes, useId } from "react";
import "../shared/Field.css";
import "../shared/Label.css";
import "./Select.css";

export interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}

export interface SelectProps
  extends Omit<SelectHTMLAttributes<HTMLSelectElement>, "value" | "onChange" | "children"> {
  label: string;
  options: readonly SelectOption[];
  value: string;
  onChange: (value: string) => void;
  help?: string;
  invalid?: boolean;
}

export function Select({
  label,
  options,
  value,
  onChange,
  help,
  invalid,
  className,
  id,
  "aria-describedby": describedBy,
  ...rest
}: SelectProps): ReactElement {
  const generated = useId();
  const selectId = id || generated;
  const helpId = `${selectId}-help`;
  const describedByIds = [describedBy, help ? helpId : undefined].filter(Boolean).join(" ");
  const classes = ["vt-field", "vt-select", invalid && "vt-field-invalid", className]
    .filter(Boolean)
    .join(" ");
  return (
    <div className={classes}>
      <label className="vt-label" htmlFor={selectId}>
        {label}
      </label>
      <select
        {...rest}
        id={selectId}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        aria-invalid={invalid ? true : rest["aria-invalid"]}
        aria-describedby={describedByIds || undefined}
      >
        {options.map((o) => (
          <option key={o.value} value={o.value} disabled={o.disabled}>
            {o.label}
          </option>
        ))}
      </select>
      {help ? (
        <span className="vt-help" id={helpId}>
          {help}
        </span>
      ) : null}
    </div>
  );
}
