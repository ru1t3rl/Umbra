import { type ReactElement, type ReactNode, useId } from "react";
import "./Panel.css";

export interface PanelProps {
  title: string;
  /** Header-right slot: icon Buttons (size sm, ghost). */
  actions?: ReactNode;
  /** 320px by default; 400px for tables. A number is pixels. */
  width?: number | string;
  className?: string;
  children?: ReactNode;
}

export function Panel({ title, actions, width, className, children }: PanelProps): ReactElement {
  const titleId = useId();
  const classes = ["vt-panel", className].filter(Boolean).join(" ");
  return (
    <section className={classes} aria-labelledby={titleId} style={width ? { width } : undefined}>
      <header className="vt-panel-head">
        <h2 className="vt-panel-title" id={titleId}>
          {title}
        </h2>
        {actions}
      </header>
      <div className="vt-panel-body">{children}</div>
    </section>
  );
}
