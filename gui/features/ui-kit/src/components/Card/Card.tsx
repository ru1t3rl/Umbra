import type { ReactElement, ReactNode } from "react";
import "./Card.css";

export interface CardProps {
  title?: string;
  subtitle?: string;
  /** Tonal level: 1 = surface-1, 2 = surface-2 (default), 3 = surface-3. */
  level?: 1 | 2 | 3;
  variant?: "tonal" | "outline";
  className?: string;
  children?: ReactNode;
}

export function Card({
  title,
  subtitle,
  level = 2,
  variant,
  className,
  children,
}: CardProps): ReactElement {
  const classes = [
    "vt-card",
    level === 1 && "vt-card-1",
    level === 3 && "vt-card-3",
    variant === "outline" && "vt-card-outline",
    className,
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <div className={classes}>
      {title ? <h3 className="vt-card-title">{title}</h3> : null}
      {subtitle ? <p className="vt-card-sub">{subtitle}</p> : null}
      {children}
    </div>
  );
}
