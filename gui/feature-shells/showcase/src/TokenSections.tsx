import {
  colorTokens,
  cssVar,
  radiusTokens,
  shadowTokens,
  spacingTokens,
  type TypeStyle,
  typeStyles,
} from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "./messages.ts";
import { Section } from "./Section.tsx";

export function TokenSections(): ReactElement {
  return (
    <div id="showcase-tokens" className="showcase-group">
      <Section id="colour" heading={t("colour.heading")}>
        <ul className="showcase-swatches">
          {colorTokens.map((name) => (
            <li key={name} className="showcase-swatch">
              <span className="showcase-swatch-chip" style={{ background: cssVar(name) }} />
              <span className="type-data">{name}</span>
            </li>
          ))}
        </ul>
      </Section>
      <Section id="type" heading={t("type.heading")}>
        <ul className="showcase-specimens">
          {typeStyles.map((name: TypeStyle) => (
            <li key={name} className="showcase-specimen">
              <span className="type-data showcase-token-name">{name}</span>
              <span className={`type-${name}`}>{t("type.sample")}</span>
            </li>
          ))}
        </ul>
      </Section>
      <Section id="spacing" heading={t("spacing.heading")}>
        <ul className="showcase-scale">
          {spacingTokens.map((name) => (
            <li key={name} className="showcase-scale-row">
              <span className="type-data showcase-token-name">{name}</span>
              <span className="showcase-space-bar" style={{ width: cssVar(name) }} />
            </li>
          ))}
        </ul>
      </Section>
      <Section id="radius" heading={t("radius.heading")}>
        <ul className="showcase-radii">
          {radiusTokens.map((name) => (
            <li key={name} className="showcase-radius">
              <span className="showcase-radius-box" style={{ borderRadius: cssVar(name) }} />
              <span className="type-data">{name}</span>
            </li>
          ))}
        </ul>
      </Section>
      <Section id="shadow" heading={t("shadow.heading")}>
        <ul className="showcase-shadows">
          {shadowTokens.map((name) => (
            <li key={name} className="showcase-shadow">
              <span className="showcase-shadow-box" style={{ boxShadow: cssVar(name) }} />
              <span className="type-data">{name}</span>
            </li>
          ))}
          <li className="showcase-shadow">
            <span className="showcase-backdrop">
              <span className="showcase-glass-box" />
            </span>
            <span className="type-data">{t("shadow.glass")}</span>
          </li>
        </ul>
      </Section>
    </div>
  );
}
