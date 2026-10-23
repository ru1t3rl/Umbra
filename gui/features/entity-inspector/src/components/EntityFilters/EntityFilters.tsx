import { Chip, ChipGroup, TextField } from "@umbra/ui-kit";
import type { ReactElement } from "react";
import { t } from "../../messages.ts";
import type { EntityChips } from "../../model/filterEntities.ts";

export interface EntityFiltersProps {
  query: string;
  onQueryChange: (query: string) => void;
  chips: EntityChips;
  onChipsChange: (chips: EntityChips) => void;
  /** The forces present in the frame, one chip each. */
  forces: readonly string[];
}

/** The search field and the filter chips (pure presentation: the state is the panel's). */
export function EntityFilters({
  query,
  onQueryChange,
  chips,
  onChipsChange,
  forces,
}: EntityFiltersProps): ReactElement {
  const toggleForce = (force: string): void =>
    onChipsChange({
      ...chips,
      forces: chips.forces.includes(force)
        ? chips.forces.filter((f) => f !== force)
        : [...chips.forces, force],
    });
  return (
    <>
      <TextField
        label={t("list.search")}
        value={query}
        onChange={(e) => onQueryChange(e.currentTarget.value)}
      />
      <ChipGroup label={t("list.filters")}>
        {forces.map((force) => (
          <Chip
            key={force}
            selected={chips.forces.includes(force)}
            onClick={() => toggleForce(force)}
          >
            {force}
          </Chip>
        ))}
        <Chip
          selected={chips.owned}
          onClick={() => onChipsChange({ ...chips, owned: !chips.owned })}
        >
          {t("list.owned")}
        </Chip>
        <Chip
          selected={chips.remote}
          onClick={() => onChipsChange({ ...chips, remote: !chips.remote })}
        >
          {t("list.remote")}
        </Chip>
      </ChipGroup>
    </>
  );
}
