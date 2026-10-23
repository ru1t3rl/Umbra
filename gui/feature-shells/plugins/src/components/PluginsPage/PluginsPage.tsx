import { FaultSummary, PluginStatusGate } from "@umbra/plugin-status";
import { AppNav, CommandBar, Tabs, ThemeToggle } from "@umbra/ui-kit";
import { type ReactElement, useState } from "react";
import { t } from "../../messages.ts";
import { type PluginsTab, TABS_ID } from "../../types.ts";
import { PluginsTabPanel } from "../PluginsTabPanel/PluginsTabPanel.tsx";
import "./PluginsPage.css";

const TAB_ITEMS = [
  { value: "plugins", label: t("tab.plugins") },
  { value: "adapters", label: t("tab.adapters") },
  { value: "components", label: t("tab.components") },
] as const;

const isTab = (value: string): value is PluginsTab => TAB_ITEMS.some((i) => i.value === value);

/**
 * Layout and wiring only (ADR 0078): a docked command bar, then the fault summary above the tabs. The idle state
 * shows once for the whole body (`PluginStatusGate`, ASM-994). `useState` holds the open tab (layout state).
 */
export default function PluginsPage(): ReactElement {
  const [tab, setTab] = useState<PluginsTab>("plugins");
  return (
    <div className="plugins">
      <CommandBar title="">
        <AppNav />
        <ThemeToggle />
      </CommandBar>
      <main className="plugins-main">
        <h1 className="vt-visually-hidden">{t("page.title")}</h1>
        <PluginStatusGate>
          {/* The cards below use h3; this keeps the heading order (axe heading-order). */}
          <h2 className="vt-visually-hidden">{t("heading.status")}</h2>
          <FaultSummary />
          <Tabs
            id={TABS_ID}
            label={t("tabs.label")}
            items={[...TAB_ITEMS]}
            value={tab}
            onChange={(value) => {
              if (isTab(value)) setTab(value);
            }}
          />
          <div
            role="tabpanel"
            id={`${TABS_ID}-panel-${tab}`}
            aria-labelledby={`${TABS_ID}-tab-${tab}`}
            // biome-ignore lint/a11y/noNoninteractiveTabindex: APG tabs: a scrolling panel with no focusable content takes focus itself (WCAG 2.1.1)
            tabIndex={0}
            className="plugins-panel"
          >
            <PluginsTabPanel tab={tab} />
          </div>
        </PluginStatusGate>
      </main>
    </div>
  );
}
