import type { ReactElement } from "react";
import { defineMessages } from "../../i18n/defineMessages.ts";
import {
  type Affiliation,
  AffiliationTag,
  affiliationLabel,
} from "../AffiliationTag/AffiliationTag.tsx";
import { Badge, type Tone } from "../Badge/Badge.tsx";
import "../shared/VisuallyHidden.css";
import "./EntityRow.css";

export interface EntityRowProps {
  name: string;
  /** DIS/HLA identifier shown in mono beside the name. */
  id?: string;
  affiliation: Affiliation;
  echelon?: string;
  /** One mono line: position, heading, speed. */
  meta?: string;
  status?: { label: string; tone: Tone };
  selected?: boolean;
  /** No fresh data: dims the row and replaces the status with the word "Stale". */
  stale?: boolean;
  onSelect?: () => void;
  className?: string;
}

const t = defineMessages({ stale: "Stale" });

/**
 * One option of an `EntityList`. An option has presentational children: never put interactive
 * content (buttons, links, inputs) in a row. Whitespace text nodes separate the parts so the accessible name
 * reads "Friend PLT Alpha 1-1 1:3:1021 ... Running" and not as one run-on word.
 */
export function EntityRow({
  name,
  id,
  affiliation,
  echelon,
  meta,
  status,
  selected,
  stale,
  onSelect,
  className,
}: EntityRowProps): ReactElement {
  const classes = ["vt-entity", stale && "vt-entity-stale", className].filter(Boolean).join(" ");
  const badge = stale ? { label: t("stale"), tone: "neutral" as const } : status;
  return (
    <button
      type="button"
      role="option"
      aria-selected={selected ? "true" : "false"}
      className={classes}
      onClick={onSelect}
    >
      <AffiliationTag affiliation={affiliation}>
        <span className="vt-visually-hidden">{affiliationLabel(affiliation)} </span>
        {echelon || "\u00A0"}
      </AffiliationTag>{" "}
      <span>
        <span className="vt-entity-name" title={name}>
          {name}
        </span>{" "}
        {id ? <span className="vt-entity-id">{id}</span> : null}{" "}
        {meta ? <span className="vt-entity-meta">{meta}</span> : null}
      </span>{" "}
      {badge ? <Badge tone={badge.tone}>{badge.label}</Badge> : null}
    </button>
  );
}
