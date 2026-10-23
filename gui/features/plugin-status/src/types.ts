/**
 * The part of a TanStack Query result the components use. Hooks return it so their declared types do not spell
 * out the generated request functions (`check:deps` rejects those names, ADR 0045).
 */
export interface QueryView<T> {
  readonly data: T | undefined;
  readonly error: unknown;
  readonly isError: boolean;
  readonly refetch: (options?: { cancelRefetch?: boolean }) => Promise<unknown>;
}
