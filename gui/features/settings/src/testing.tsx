// Test doubles for the setting editors (WI-17; not exported, exempt from check:layout): the editors mounted under
// the providers the app mounts (ThemeProvider with an in-memory store, SettingsProvider with a recording backend),
// and a probe that reads `useAffiliationOf` so a test can see an override take effect.
import { render } from "@testing-library/react";
import { ThemeProvider } from "@umbra/ui-kit";
import type { ReactElement, ReactNode } from "react";
import { SettingsProvider } from "./components/SettingsProvider/SettingsProvider.tsx";
import { useAffiliationOf } from "./hooks/useAffiliationOf.ts";
import { useSettings } from "./hooks/useSettings.ts";
import type { SettingsBackend } from "./model/backend.ts";
import type { Settings } from "./model/settings.ts";

export interface Mounted {
  /** Every settings object the backend was asked to write, in order. */
  writes: Settings[];
  /** The last written settings; the stored ones when nothing was written yet (`undefined` for no storage). */
  stored: () => Settings | undefined;
}

/** `raw` is what the backend holds before the first render (parsed by the provider, like a stored file). */
export function mountSettings(ui: ReactNode, raw?: unknown): Mounted {
  const writes: Settings[] = [];
  const backend: SettingsBackend = { read: () => raw, write: (s) => void writes.push(s) };
  const memory = new Map<string, string>();
  render(
    <ThemeProvider
      storage={{
        getItem: (k) => memory.get(k) ?? null,
        setItem: (k, v) => void memory.set(k, v),
      }}
    >
      <SettingsProvider backend={backend}>{ui}</SettingsProvider>
    </ThemeProvider>,
  );
  return { writes, stored: () => writes.at(-1) };
}

/** Prints `useAffiliationOf()(force)`, named "Affiliation of <force>". */
export function AffiliationProbe({ force }: { force: string }): ReactElement {
  const of = useAffiliationOf();
  return <output aria-label={`Affiliation of ${force}`}>{of(force)}</output>;
}

/** Prints the current settings as JSON in an output named "Settings". */
export function SettingsProbe(): ReactElement {
  return <output aria-label="Settings">{JSON.stringify(useSettings())}</output>;
}
