import type { NavItem } from "@umbra/ui-kit";

/** The e2e build reads extra navigation entries from this window global (set by a Playwright init script). */
export const E2E_NAV_KEY = "__UMBRA_E2E_NAV__";

/**
 * Extra navigation entries for the e2e build only: a test shows how the page bars behave with the entries later
 * shells will add (WI-16, WI-17). Called only behind `import.meta.env.VITE_UMBRA_E2E === "1"`, so the production
 * build contains neither this code nor the global's name (`e2eHook.build.test.ts`). Invalid entries are dropped.
 */
export function e2eNavItems(source: unknown): NavItem[] {
  if (!Array.isArray(source)) return [];
  return source.flatMap((entry): NavItem[] => {
    if (typeof entry !== "object" || entry === null) return [];
    const { path, label, icon } = entry as Record<string, unknown>;
    if (typeof path !== "string" || typeof label !== "string") return [];
    // An in-app path only: not relative, not protocol-relative.
    if (!path.startsWith("/") || path.startsWith("//")) return [];
    return [{ path, label, ...(typeof icon === "string" ? { icon } : {}) }];
  });
}
