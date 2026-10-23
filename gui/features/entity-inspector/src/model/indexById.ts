/** Index of the entity with `id` in an id-ascending list, or -1. */
export function indexById(entities: readonly { readonly id: number }[], id: number): number {
  let low = 0;
  let high = entities.length - 1;
  while (low <= high) {
    const mid = (low + high) >>> 1;
    const candidate = entities[mid];
    if (candidate === undefined) return -1;
    if (candidate.id === id) return mid;
    if (candidate.id < id) low = mid + 1;
    else high = mid - 1;
  }
  return -1;
}
