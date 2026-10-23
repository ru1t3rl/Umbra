import type { EntitySource } from "@umbra/engine-stream";
import { EntityRow, VirtualList } from "@umbra/ui-kit";
import { type ReactElement, useId, useMemo, useState } from "react";
import { useThrottledEntities } from "../../hooks/useThrottledEntities.ts";
import { t } from "../../messages.ts";
import { capitalise } from "../../model/capitalise.ts";
import { type AffiliationOf, entityRowModel } from "../../model/entityRowModel.ts";
import { type EntityChips, filterEntities, NO_CHIPS } from "../../model/filterEntities.ts";
import { indexById } from "../../model/indexById.ts";
import { EntityFilters } from "../EntityFilters/EntityFilters.tsx";
import "./EntityListPanel.css";

export interface EntityListPanelProps {
  /** The connection's entity source (`useEntitySource()`), passed by the shell. */
  source: EntitySource;
  selectedId: number | null;
  onSelect: (id: number | null) => void;
  affiliationOf: AffiliationOf;
}

/**
 * The entity list: a virtualized listbox with search and filter chips. Frames reach it through a 250 ms
 * throttle, so it renders a few times per second at any stream rate; only the visible rows exist (NFR-1).
 */
export function EntityListPanel({
  source,
  selectedId,
  onSelect,
  affiliationOf,
}: EntityListPanelProps): ReactElement {
  const entities = useThrottledEntities(source);
  const [query, setQuery] = useState("");
  const [chips, setChips] = useState<EntityChips>(NO_CHIPS);
  // The active row is tracked by entity id: a rebuild or a filter moves rows, not the keyboard position.
  const [activeId, setActiveId] = useState<number | null>(null);
  const prefix = useId();
  const shown = useMemo(() => filterEntities(entities, query, chips), [entities, query, chips]);
  // A pressed chip stays while its force is absent, so the user can see why the list is empty and release it.
  const forces = useMemo(
    () =>
      [
        ...new Set([...entities.flatMap((e) => (e.force ? [e.force] : [])), ...chips.forces]),
      ].sort(),
    [entities, chips.forces],
  );
  const activeIndex = activeId === null ? -1 : indexById(shown, activeId);
  const selectedIndex = selectedId === null ? -1 : indexById(shown, selectedId);

  return (
    <div className="umbra-entity-list" {...(shown.length === 0 ? { "data-empty": "" } : {})}>
      <EntityFilters
        query={query}
        onQueryChange={setQuery}
        chips={chips}
        onChipsChange={setChips}
        forces={forces}
      />
      <p className="umbra-entity-list-count type-caption" aria-live="polite">
        {t("list.count", { shown: shown.length, total: entities.length })}
      </p>
      {shown.length === 0 ? (
        <p className="umbra-entity-list-empty type-body">
          {entities.length === 0 ? t("list.empty") : t("list.noMatch")}
        </p>
      ) : null}
      <VirtualList
        role="listbox"
        label={t("list.label")}
        count={shown.length}
        rowHeight="row-lg"
        getRowId={(i) => `${prefix}-${shown[i]?.id ?? i}`}
        activeIndex={activeIndex < 0 ? null : activeIndex}
        selectedIndex={selectedIndex < 0 ? null : selectedIndex}
        onActiveIndexChange={(i) => setActiveId(i === null ? null : (shown[i]?.id ?? null))}
        onActivate={(i) => {
          const entity = shown[i];
          if (entity) onSelect(entity.id);
        }}
        className="umbra-entity-list-rows"
        renderRow={(i) => {
          const entity = shown[i];
          if (!entity) return null;
          const row = entityRowModel(entity, affiliationOf);
          return (
            <EntityRow
              presentational
              name={row.name}
              affiliation={row.affiliation}
              {...(row.idLabel === null ? {} : { id: row.idLabel })}
              {...(row.meta === null ? {} : { meta: row.meta })}
              {...(row.status === null
                ? {}
                : { status: { label: capitalise(row.status), tone: "neutral" } })}
              {...(row.remote ? { badges: [{ label: t("list.remote"), tone: "info" }] } : {})}
            />
          );
        }}
      />
    </div>
  );
}
