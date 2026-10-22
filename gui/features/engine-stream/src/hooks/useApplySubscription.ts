import { useEffect, useMemo } from "react";
import { stableKey } from "../model/stableKey.ts";
import type { Subscription } from "../types.ts";
import { useConnection } from "./useConnection.ts";

/**
 * Applies `subscription` on mount and whenever its value changes (deep equality, so an inline literal is
 * fine). Nothing is reverted on unmount: the connection keeps the last applied subscription.
 */
export function useApplySubscription(subscription: Subscription): void {
  const connection = useConnection();
  const key = stableKey(subscription);
  // Keyed by the stable key: the memoized object changes identity only when the value does, so the effect re-runs only then.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `key` is the value identity of `subscription`.
  const stable = useMemo(() => subscription, [key]);
  useEffect(() => {
    connection.setSubscription(stable);
  }, [connection, stable]);
}
