import type { Affiliation } from "@umbra/ui-kit";
import { useMemo } from "react";
import { defaultAffiliationOf } from "../model/affiliation.ts";
import { useSettings } from "./useSettings.ts";

/** Override (trimmed, case-insensitive) first, then the ASM-305 table. Stable while overrides are unchanged. */
export function useAffiliationOf(): (force: string | null | undefined) => Affiliation {
  const overrides = useSettings().map.affiliationOverrides;
  return useMemo(
    () => (force) => {
      if (typeof force === "string") {
        const key = force.trim().toLowerCase();
        const hit = Object.hasOwn(overrides, key) ? overrides[key] : undefined;
        if (hit !== undefined) return hit;
      }
      return defaultAffiliationOf(force);
    },
    [overrides],
  );
}
