/** The name of an entity, or `#id` when it has none (null and empty count as none). */
export function entityName(entity: {
  readonly id: number;
  readonly name?: string | null | undefined;
}): string {
  return entity.name ? entity.name : `#${entity.id}`;
}
