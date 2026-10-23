import { TemplateDetail, TemplateList, useTemplateSelection } from "@umbra/template-browser";
import { AppNav, CommandBar, ThemeToggle } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "../../messages.ts";
import "./TemplatesPage.css";

/**
 * Layout and wiring only (ADR 0078): a docked command bar (page navigation, theme), then two columns, the template
 * list (320 px) and the detail of the selected template. `useTemplateSelection` holds the selection (it follows the scenario, ASM-993).
 */
export default function TemplatesPage(): ReactElement {
  const { selectedId, select } = useTemplateSelection();
  return (
    <div className="templates">
      <CommandBar title="">
        <AppNav />
        <ThemeToggle />
      </CommandBar>
      <main className="templates-main">
        <h1 className="vt-visually-hidden">{t("page.title")}</h1>
        <section className="templates-list" aria-label={t("list.label")}>
          <TemplateList selectedId={selectedId} onSelect={select} />
        </section>
        <section className="templates-detail" aria-label={t("detail.label")}>
          {selectedId === null ? (
            <p className="type-body templates-empty">{t("detail.empty")}</p>
          ) : (
            <TemplateDetail templateId={selectedId} />
          )}
        </section>
      </main>
    </div>
  );
}
