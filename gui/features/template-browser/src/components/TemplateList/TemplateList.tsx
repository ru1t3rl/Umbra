import { useListTemplates } from "@umbra/data";
import { TextField, VirtualList } from "@umbra/ui-kit";
import { type ReactElement, useId, useMemo, useState } from "react";
import { useScenarioIdle } from "../../hooks/useScenarioIdle.ts";
import { t } from "../../messages.ts";
import { filterTemplates } from "../../model/filterTemplates.ts";
import { IdleState } from "../IdleState/IdleState.tsx";
import { ProblemAlert } from "../ProblemAlert/ProblemAlert.tsx";
import { TemplateRow } from "../TemplateRow/TemplateRow.tsx";
import "./TemplateList.css";

export interface TemplateListProps {
  /** The chosen template id, or null. */
  selectedId: string | null;
  onSelect: (id: string) => void;
}
/** Up to this many (filtered) templates the VirtualList is unwindowed (every row mounted), above it windowed (ASM-862, ASM-953). */
/** Up to this many (filtered) templates the list is a plain listbox, above it a VirtualList (ASM-862). */
export const PLAIN_LIST_MAX = 50;

/** The templates of the loaded scenario: a searchable listbox with the id, description, motion model and behavior. */
export function TemplateList({ selectedId, onSelect }: TemplateListProps): ReactElement {
  const idle = useScenarioIdle();
  const query = useListTemplates({ query: { enabled: !idle } });
  const [text, setText] = useState("");
  // The active row is tracked by template id: a filter moves rows, not the keyboard position.
  const [activeId, setActiveId] = useState<string | null>(null);
  const prefix = useId();
  const all = query.data;
  const shown = useMemo(() => filterTemplates(all ?? [], text), [all, text]);
  if (idle) return <IdleState />;

  const indexOf = (id: string | null): number =>
    id === null ? -1 : shown.findIndex((tpl) => tpl.id === id);
  const activeIndex = indexOf(activeId);
  const selectedIndex = indexOf(selectedId);
  // By position, not by template id: ids may hold spaces or quotes, which are not valid in an id reference.
  const rowId = (i: number): string => `${prefix}-row-${i}`;
  const renderRow = (i: number): ReactElement | null => {
    const template = shown[i];
    return template ? <TemplateRow template={template} /> : null;
  };
  const listProps = {
    label: t("list.label"),
    count: shown.length,
    activeIndex: activeIndex < 0 ? null : activeIndex,
    selectedIndex: selectedIndex < 0 ? null : selectedIndex,
    getRowId: rowId,
    renderRow,
    onActiveIndexChange: (i: number | null) =>
      setActiveId(i === null ? null : (shown[i]?.id ?? null)),
    onActivate: (i: number) => {
      const template = shown[i];
      if (template) onSelect(template.id);
    },
  };

  return (
    <div className="umbra-template-list">
      <TextField
        label={t("list.search")}
        value={text}
        onChange={(e) => setText(e.target.value)}
        autoComplete="off"
      />
      {query.isError ? <ProblemAlert error={query.error} fallbackTitle={t("list.failed")} /> : null}
      {query.isSuccess ? (
        <p className="umbra-template-list-count type-caption" role="status" aria-live="polite">
          {t("list.count", { shown: shown.length, total: all?.length ?? 0 })}
        </p>
      ) : null}
      {query.isSuccess && shown.length === 0 ? (
        <p className="umbra-template-list-empty type-body">
          {(all?.length ?? 0) === 0 ? t("list.empty") : t("list.noMatch")}
        </p>
      ) : null}
      {query.isSuccess ? (
        <VirtualList
          role="listbox"
          rowHeight="row-lg"
          virtualize={shown.length > PLAIN_LIST_MAX}
          className="umbra-template-list-rows"
          {...listProps}
        />
      ) : null}
    </div>
  );
}
