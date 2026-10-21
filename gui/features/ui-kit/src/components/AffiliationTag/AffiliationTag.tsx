import type { ReactElement, ReactNode } from "react";
import { defineMessages } from "../../i18n/defineMessages.ts";
import "./AffiliationTag.css";

export type Affiliation = "friend" | "hostile" | "neutral" | "unknown";

export interface AffiliationTagProps {
  affiliation: Affiliation;
  /** Replaces the default word with an echelon or short code ("BN", "2 MECH"). */
  children?: ReactNode;
  className?: string;
}

const t = defineMessages({
  friend: "Friend",
  hostile: "Hostile",
  neutral: "Neutral",
  unknown: "Unknown",
});

const DIAMOND = "M10 1.5 L18.5 10 L10 18.5 L1.5 10 Z";
const QUATREFOIL =
  "M6 2.5 h8 a4 4 0 0 1 4 4 v8 a4 4 0 0 1 -4 4 h-8 a4 4 0 0 1 -4 -4 v-8 a4 4 0 0 1 4 -4 z";

/** APP-6D frame shape: rectangle friend, diamond hostile, square neutral, quatrefoil unknown. */
function Shape({ affiliation, className }: { affiliation: Affiliation; className: string }) {
  switch (affiliation) {
    case "friend":
      return (
        <rect
          className={
            className === "vt-affil-fill" ? `${className} vt-affil-frame-shape` : className
          }
          x={2}
          y={4}
          width={16}
          height={12}
          rx={1}
        />
      );
    case "hostile":
      return <path className={className} d={DIAMOND} />;
    case "neutral":
      return <rect className={className} x={3} y={3} width={14} height={14} />;
    case "unknown":
      return <path className={className} d={QUATREFOIL} />;
  }
}

export function AffiliationTag({
  affiliation,
  children,
  className,
}: AffiliationTagProps): ReactElement {
  const classes = ["vt-affil", `vt-affil-${affiliation}`, className].filter(Boolean).join(" ");
  return (
    <span className={classes} title={t(affiliation)}>
      <svg width={20} height={20} viewBox="0 0 20 20" aria-hidden="true">
        <Shape affiliation={affiliation} className="vt-affil-fill" />
        <Shape affiliation={affiliation} className="vt-affil-frame" />
      </svg>
      {children || t(affiliation)}
    </span>
  );
}
