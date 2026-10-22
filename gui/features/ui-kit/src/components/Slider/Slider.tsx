import type { InputHTMLAttributes, ReactElement } from "react";
import "../shared/Label.css";
import "./Slider.css";

export interface SliderProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "type" | "value"> {
  label: string;
  value: number;
  /** Formats the live value shown in cyan ("1:4", "60 Hz"). */
  format?: (value: number) => string;
}

export function Slider({
  label,
  value,
  format = String,
  className,
  ...rest
}: SliderProps): ReactElement {
  const shown = format(value);
  const classes = ["vt-slider-wrap", className].filter(Boolean).join(" ");
  return (
    <div className={classes}>
      <div className="vt-slider-head">
        <span className="vt-label">{label}</span>
        <span className="vt-slider-value">{shown}</span>
      </div>
      <input
        type="range"
        className="vt-slider"
        aria-label={label}
        aria-valuetext={shown}
        {...rest}
        value={value}
      />
    </div>
  );
}
