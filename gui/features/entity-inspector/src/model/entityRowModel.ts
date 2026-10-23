import type { EntitySummary } from "@umbra/engine-stream";
import type { Affiliation } from "@umbra/ui-kit";
import { entityName } from "./entityName.ts";

export interface EntityRowModel {
  readonly id: number;
  /** The name, or `#id` when there is none. */
  readonly name: string;
  /** `#id` beside a name; null when the name already is the id. */
  readonly idLabel: string | null;
  readonly affiliation: Affiliation;
  /** The template. */
  readonly meta: string | null;
  /** The behavior status word. */
  readonly status: string | null;
  readonly remote: boolean;
}

export type AffiliationOf = (force: string | null | undefined) => Affiliation;

export function entityRowModel(
  entity: EntitySummary,
  affiliationOf: AffiliationOf,
): EntityRowModel {
  const named = Boolean(entity.name);
  return {
    id: entity.id,
    name: entityName(entity),
    idLabel: named ? `#${entity.id}` : null,
    affiliation: affiliationOf(entity.force),
    meta: entity.template ? entity.template : null,
    status: entity.status ? entity.status : null,
    remote: entity.ownership === "remote",
  };
}
