/** Reconnect delays in milliseconds: 0.5, 1, 2, 4, 8 s, then 8 s (ASM-315). */
export const BACKOFF_MS: readonly number[] = [500, 1000, 2000, 4000, 8000];

export function backoffDelay(attempt: number): number {
  const i = Math.min(Math.max(0, Math.trunc(attempt)), BACKOFF_MS.length - 1);
  return BACKOFF_MS[i] ?? 8000;
}
