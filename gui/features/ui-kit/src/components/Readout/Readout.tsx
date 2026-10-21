import type { ReactElement, ReactNode } from "react";
import { defineMessages } from "../../i18n/defineMessages.ts";
import { Icon } from "../Icon/Icon.tsx";
import "../shared/Label.css";
import "../shared/VisuallyHidden.css";
import "./Readout.css";

type Trend = "up" | "down" | "flat";

interface ReadoutBaseProps {
  label: string;
  value: ReactNode;
  unit?: string;
  /** Cyan value: changes at runtime. */
  live?: boolean;
  className?: string;
}

/** A trend describes a delta, so it only exists together with one. */
type ReadoutDeltaProps =
  | { delta: string; trend?: Trend }
  | { delta?: undefined; trend?: undefined };

export type ReadoutProps = ReadoutBaseProps & ReadoutDeltaProps;

const t = defineMessages({ up: "Increasing", down: "Decreasing", flat: "Unchanged" });
const GLYPH: Record<Trend, string> = {
  up: "trending_up",
  down: "trending_down",
  flat: "trending_flat",
};
const THIN_SPACE = " ";

export function Readout({
  label,
  value,
  unit,
  live,
  delta,
  trend,
  className,
}: ReadoutProps): ReactElement {
  const classes = ["vt-readout", live && "vt-readout-live", className].filter(Boolean).join(" ");
  const deltaClasses = [
    "vt-readout-delta",
    trend === "up" && "is-up",
    trend === "down" && "is-down",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <dl className={classes}>
      <dt className="vt-label">{label}</dt>
      <dd className="vt-readout-value">
        {value}
        {unit ? (
          <>
            {THIN_SPACE}
            <span className="vt-readout-unit">{unit}</span>
          </>
        ) : null}
      </dd>
      {delta ? (
        <dd className={deltaClasses}>
          {trend ? (
            <>
              <Icon name={GLYPH[trend]} />
              <span className="vt-visually-hidden">{t(trend)} </span>
            </>
          ) : null}
          {delta}
        </dd>
      ) : null}
    </dl>
  );
}
