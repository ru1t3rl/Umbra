import type { ReactElement, ReactNode } from "react";
import "./Alert.css";

export type AlertTone = "info" | "success" | "warning" | "danger";

export interface AlertProps {
  tone?: AlertTone;
  title: string;
  /** A ghost or tonal Button, size sm. */
  action?: ReactNode;
  className?: string;
  children?: ReactNode;
}

const GLYPH: Record<AlertTone, string> = { warning: "!", danger: "×", success: "✓", info: "i" };

export function Alert({
  tone = "info",
  title,
  action,
  className,
  children,
}: AlertProps): ReactElement {
  const classes = ["vt-alert", `vt-alert-${tone}`, className].filter(Boolean).join(" ");
  return (
    <div className={classes} role={tone === "danger" || tone === "warning" ? "alert" : "status"}>
      <span className="vt-alert-icon" aria-hidden="true">
        {GLYPH[tone]}
      </span>
      <div className="vt-alert-text">
        <div className="vt-alert-title">{title}</div>
        {children ? <div className="vt-alert-body">{children}</div> : null}
      </div>
      {action}
    </div>
  );
}
