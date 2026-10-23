import type { EntitySummary } from "@umbra/engine-stream";

export interface EntityChips {
  /** Forces to keep (OR within). Empty keeps all. */
  readonly forces: readonly string[];
  /** Keep owned entities. With `remote` both are kept (OR); neither keeps all. */
  readonly owned: boolean;
  readonly remote: boolean;
}

export const NO_CHIPS: EntityChips = { forces: [], owned: false, remote: false };

/**
 * Text (trimmed, case-insensitive; name, template and id with or without `#`) AND the force chips AND the
 * ownership chips. Returns the same array when there is nothing to filter.
 */
export function filterEntities(
  entities: readonly EntitySummary[],
  query: string,
  chips: EntityChips,
): readonly EntitySummary[] {
  const text = query.trim().toLowerCase();
  const byOwnership = chips.owned || chips.remote;
  if (text === "" && chips.forces.length === 0 && !byOwnership) return entities;
  const idText = text.startsWith("#") ? text.slice(1) : text;
  return entities.filter((e) => {
    if (chips.forces.length > 0 && (e.force == null || !chips.forces.includes(e.force)))
      return false;
    if (byOwnership) {
      const keep =
        (chips.owned && e.ownership === "owned") || (chips.remote && e.ownership === "remote");
      if (!keep) return false;
    }
    if (text === "") return true;
    return (
      (e.name ?? "").toLowerCase().includes(text) ||
      (e.template ?? "").toLowerCase().includes(text) ||
      String(e.id).includes(idText)
    );
  });
}
