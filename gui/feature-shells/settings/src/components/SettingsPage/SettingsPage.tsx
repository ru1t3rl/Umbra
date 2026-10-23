import { EngineConnectionSettings } from "@umbra/desktop";
import {
  AffiliationOverridesEditor,
  JournalCapacitySetting,
  RecentScenariosSetting,
  StreamRateSetting,
  ThemePreferenceSetting,
} from "@umbra/settings";
import { AppNav, Card, CommandBar, ThemeToggle } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "../../messages.ts";
import { AboutSection } from "../AboutSection/AboutSection.tsx";
import "./SettingsPage.css";

/**
 * Layout and wiring only (ADR 0078): a docked command bar, then one Card per section, each laying out an editor
 * of `@umbra/settings` or `@umbra/desktop`. The stored stream rate is applied by the app, not by this page
 * (ASM-997); the page is not gated on the run state, settings are app-level (ASM-995).
 */
export default function SettingsPage(): ReactElement {
  return (
    <div className="settings">
      <CommandBar title="">
        <AppNav />
        <ThemeToggle />
      </CommandBar>
      <main className="settings-main">
        <h1 className="vt-visually-hidden">{t("page.title")}</h1>
        {/* The cards below use h3; this keeps the heading order (axe heading-order). */}
        <h2 className="vt-visually-hidden">{t("heading.settings")}</h2>
        <div className="settings-cards">
          <Card title={t("card.engine")}>
            <EngineConnectionSettings />
          </Card>
          <Card title={t("card.appearance")}>
            <ThemePreferenceSetting />
            <ThemeToggle label={t("appearance.switch")} />
          </Card>
          <Card title={t("card.live")}>
            <StreamRateSetting />
          </Card>
          <Card title={t("card.map")}>
            <AffiliationOverridesEditor />
          </Card>
          <Card title={t("card.log")}>
            <JournalCapacitySetting />
          </Card>
          <Card title={t("card.scenarios")}>
            <RecentScenariosSetting />
          </Card>
          <Card title={t("card.about")}>
            <AboutSection />
          </Card>
        </div>
      </main>
    </div>
  );
}
