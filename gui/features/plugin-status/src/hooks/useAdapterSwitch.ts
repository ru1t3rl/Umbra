import { useDisableNetworkAdapter, useEnableNetworkAdapter } from "@umbra/data";
import { useRunStatus } from "@umbra/engine-stream";
import { useCallback, useEffect, useState } from "react";

export interface AdapterSwitch {
  /** Names whose request (including the read that follows) is in flight. */
  readonly busy: ReadonlySet<string>;
  /** The last failure per adapter name. */
  readonly errors: ReadonlyMap<string, unknown>;
  /** Enable (`next` true) or disable the adapter, then read the list again; no optimistic state (ASM-959). */
  toggle(name: string, next: boolean): Promise<void>;
}

/** Enable and disable for the adapter cards. `reload` reads the adapter list again after a success. */
export function useAdapterSwitch(reload: () => Promise<unknown>): AdapterSwitch {
  // One mutation observer per direction serves every adapter: the calls go through `mutateAsync` and the
  // per-adapter state (busy, failure) is kept here by name, so concurrent switches of different adapters do not
  // share a pending or error state (the observer's own `isPending` and `error` belong to its latest call only).
  const enable = useEnableNetworkAdapter();
  const disable = useDisableNetworkAdapter();
  const [busy, setBusy] = useState<ReadonlySet<string>>(new Set());
  const [errors, setErrors] = useState<ReadonlyMap<string, unknown>>(new Map());
  const session = useRunStatus()?.session;
  // A new session is a new scenario: the failures of the old one no longer apply (ASM-967).
  // biome-ignore lint/correctness/useExhaustiveDependencies: `session` is the trigger, not a value used inside
  useEffect(() => setErrors(new Map()), [session]);
  const { mutateAsync: enableAsync } = enable;
  const { mutateAsync: disableAsync } = disable;

  const toggle = useCallback(
    async (name: string, next: boolean) => {
      setBusy((b) => new Set(b).add(name));
      setErrors((e) => {
        const copy = new Map(e);
        copy.delete(name);
        return copy;
      });
      try {
        await (next ? enableAsync : disableAsync)({ name });
        await reload();
      } catch (error) {
        setErrors((e) => new Map(e).set(name, error));
      } finally {
        setBusy((b) => {
          const copy = new Set(b);
          copy.delete(name);
          return copy;
        });
      }
    },
    [enableAsync, disableAsync, reload],
  );
  return { busy, errors, toggle };
}
