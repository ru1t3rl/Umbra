import { CommandBar, ThemeToggle } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { ComponentSections } from "./ComponentSections.tsx";
import { FloatingSample } from "./FloatingSample.tsx";
import { t } from "./messages.ts";
import "./ShowcasePage.css";
import { TokenSections } from "./TokenSections.tsx";

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
        <FloatingSample />
      </main>
    </>
  );
}
