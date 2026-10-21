import type { ReactElement, ReactNode } from "react";
import "./Badge.css";

export type Tone = "neutral" | "success" | "warning" | "danger" | "info";

export interface BadgeProps {
  tone?: Tone;
  className?: string;
  children?: ReactNode;
}

export function Badge({ tone, className, children }: BadgeProps): ReactElement {
  const classes = ["vt-badge", tone && tone !== "neutral" && `vt-badge-${tone}`, className]
    .filter(Boolean)
    .join(" ");
  return <span className={classes}>{children}</span>;
}
