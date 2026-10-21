import { Badge, Chip, CommandBar, Panel } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "./messages.ts";
import { Section } from "./Section.tsx";

/**
 * A floating (glass) CommandBar with a glass Chip and a Panel inside, over a map-like backdrop. The CSS
 * must drop the inner blur (no glass on glass, ASM-337 (2)); the screenshot test guards it.
 */
export function FloatingSample(): ReactElement {
  return (
    <Section id="floating" heading={t("floating.heading")}>
      <div id="showcase-floating" className="showcase-backdrop showcase-map">
        <CommandBar
          floating
          title={t("sample.northern-shield")}
          subtitle={t("sample.review")}
          clock={t("sample.clock-review")}
          paused
        >
          <Badge tone="info">{t("sample.paused")}</Badge>
          <Chip glass icon="timeline" selected>
            {t("sample.tracks")}
          </Chip>
          <Chip glass icon="grid_on">
            {t("sample.mgrs-grid")}
          </Chip>
          <Panel title={t("sample.layers")} width={240}>
            <p className="showcase-muted">{t("sample.tracks-and-grid")}</p>
          </Panel>
        </CommandBar>
      </div>
    </Section>
  );
}
