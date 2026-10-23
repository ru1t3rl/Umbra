import { CommandBar, ThemeToggle } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "../../messages.ts";
import { ComponentSections } from "../ComponentSections/ComponentSections.tsx";
import { FloatingSample } from "../FloatingSample/FloatingSample.tsx";
import { NavDemo } from "../NavDemo/NavDemo.tsx";
import { Section } from "../Section/Section.tsx";
import "./ShowcasePage.css";
import { TableCellsDemo } from "../TableCellsDemo/TableCellsDemo.tsx";
import { TokenSections } from "../TokenSections/TokenSections.tsx";
import { VirtualListDemo } from "../VirtualListDemo/VirtualListDemo.tsx";

/** Layout only: the design-system reference page (A-003). Route component, lazy-loaded. */
export default function ShowcasePage(): ReactElement {
  return (
    <>
      <CommandBar title={t("page.title")} subtitle={t("page.subtitle")}>
        <ThemeToggle />
      </CommandBar>
      <main className="showcase-page">
        <h1 className="type-display-lg">{t("page.title")}</h1>
        <h2 className="type-headline">{t("tokens.heading")}</h2>
        <TokenSections />
        <h2 className="type-headline">{t("components.heading")}</h2>
        <ComponentSections />
        <div id="showcase-m8" className="showcase-group">
          <Section id="navigation" heading={t("nav.heading")}>
            <NavDemo />
          </Section>
          <Section id="table-cells" heading={t("cells.heading")}>
            <TableCellsDemo />
          </Section>
          <Section id="virtual-list" heading={t("vlist.heading")}>
            <VirtualListDemo />
          </Section>
        </div>
        <FloatingSample />
      </main>
    </>
  );
}
