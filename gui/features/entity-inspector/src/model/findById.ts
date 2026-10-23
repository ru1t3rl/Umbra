import { indexById } from "./indexById.ts";

/** Binary search in an id-ascending list (10,000 entities: about 14 reads). */
export function findById<T extends { readonly id: number }>(
  entities: readonly T[],
  id: number,
): T | undefined {
  const index = indexById(entities, id);
  return index < 0 ? undefined : entities[index];
}
