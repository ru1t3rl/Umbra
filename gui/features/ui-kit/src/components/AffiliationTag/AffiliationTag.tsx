import type { ReactElement, ReactNode } from "react";
import { defineMessages } from "../../i18n/defineMessages.ts";
import type { Affiliation } from "../../types.ts";
import { Shape } from "../Shape/Shape.tsx";
import "./AffiliationTag.css";

export type { Affiliation };

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

/** The default word for an affiliation ("Friend"), for places that show only the symbol. */
export function affiliationLabel(affiliation: Affiliation): string {
  return t(affiliation);
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
