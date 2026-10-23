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

/** An extra non-interactive badge after the status (for example "Remote"). */
export interface EntityRowBadge {
  label: string;
  tone?: Tone;
}

interface EntityRowBaseProps {
  name: string;
  /** DIS/HLA identifier shown in mono beside the name. */
  id?: string;
  affiliation: Affiliation;
  echelon?: string;
  /** One mono line: position, heading, speed. */
  meta?: string;
  status?: { label: string; tone: Tone };
  /** No fresh data: dims the row and replaces the status with the word "Stale". */
  stale?: boolean;
  className?: string;
  /** Badges after the status, in order. Data, not markup: a row never holds interactive content (ASM-880). */
  badges?: readonly EntityRowBadge[];
}

export interface EntityRowProps extends EntityRowBaseProps {
  selected?: boolean;
  onSelect?: () => void;
  /**
   * Content only: no button and no `option` role, for a parent that owns the option (a `VirtualList` row,
   * ASM-939). `selected` and `onSelect` are the parent's then and are ignored.
   */
  presentational?: boolean | undefined;
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
  badges,
  presentational,
}: EntityRowProps): ReactElement {
  const classes = [
    "vt-entity",
    presentational && "vt-entity-static",
    stale && "vt-entity-stale",
    className,
  ]
    .filter(Boolean)
    .join(" ");
  const badge = stale ? { label: t("stale"), tone: "neutral" as const } : status;
  const content = (
    <>
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
      {badge || badges?.length ? (
        <span className="vt-entity-badges">
          {badge ? <Badge tone={badge.tone}>{badge.label}</Badge> : null}
          {badges?.map((b) => (
            <Badge key={b.label} {...(b.tone ? { tone: b.tone } : {})}>
              {b.label}
            </Badge>
          ))}
        </span>
      ) : null}
    </>
  );
  if (presentational) return <div className={classes}>{content}</div>;
  return (
    <button
      type="button"
      role="option"
      aria-selected={selected ? "true" : "false"}
      className={classes}
      onClick={onSelect}
    >
      {content}
    </button>
  );
}
