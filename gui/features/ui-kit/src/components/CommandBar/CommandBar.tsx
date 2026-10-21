import type { ReactElement, ReactNode } from "react";
import { defineMessages } from "../../i18n/defineMessages.ts";
import "./CommandBar.css";

interface CommandBarBaseProps {
  /** The product name; falls back to "Umbra". */
  brand?: string;
  title: string;
  subtitle?: string;
  /** Glass, radius-xl, floats over the map. */
  floating?: boolean;
  className?: string;
  /** Right-hand actions: Chips, Buttons, Toggle. */
  children?: ReactNode;
}

/**
 * `paused` only makes sense with a clock to pause. Pair it with a visible word (a "Resume" button
 * label or a "Paused" Badge in the children): the grey clock must never be the only signal.
 */
type CommandBarClockProps =
  | {
      /** Scenario clock text, e.g. "14:32:07Z". Cyan while running. */
      clock: string;
      paused?: boolean;
    }
  | { clock?: undefined; paused?: undefined };

export type CommandBarProps = CommandBarBaseProps & CommandBarClockProps;

const t = defineMessages({
  brand: "Umbra",
  clock: "Scenario clock",
  clockPaused: "Scenario clock, paused",
});

/** The page banner. Render it directly in the page body: nested in `main` or `section` it stops being a banner landmark. */
export function CommandBar({
  brand,
  title,
  subtitle,
  clock,
  paused,
  floating,
  className,
  children,
}: CommandBarProps): ReactElement {
  const classes = ["vt-cmdbar", floating && "vt-cmdbar-floating", className]
    .filter(Boolean)
    .join(" ");
  return (
    <header className={classes}>
      <span className="vt-cmdbar-brand">{brand || t("brand")}</span>
      <div className="vt-cmdbar-title">
        <b>{title}</b> {subtitle ? <span>{subtitle}</span> : null}
      </div>
      {clock ? (
        <span
          role="timer"
          aria-label={paused ? t("clockPaused") : t("clock")}
          className={["vt-cmdbar-clock", paused && "is-paused"].filter(Boolean).join(" ")}
        >
          {clock}
        </span>
      ) : null}
      <div className="vt-cmdbar-actions">{children}</div>
    </header>
  );
}
