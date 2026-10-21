import type { ChildProcess } from "node:child_process";

const KILL_AFTER_MS = 5_000;
const REGISTRY = Symbol.for("umbra.e2e.children");

/** Children started by global-setup, stopped by global-teardown (kept on globalThis, not module state). */
export function childRegistry(): ChildProcess[] {
  const holder = globalThis as unknown as Record<symbol, ChildProcess[] | undefined>;
  const existing = holder[REGISTRY];
  if (existing) return existing;
  const created: ChildProcess[] = [];
  holder[REGISTRY] = created;
  return created;
}

/** SIGTERM to every registered child, SIGKILL after 5 s; resolves when all have exited. */
export async function stopChildren(): Promise<void> {
  await stopProcesses(childRegistry());
}

/** The same for any list (the worker-scoped engine fixture owns its own); the list is emptied. */
export async function stopProcesses(children: ChildProcess[]): Promise<void> {
  await Promise.all(
    children.map(
      (child) =>
        new Promise<void>((done) => {
          if (child.exitCode !== null || child.signalCode !== null) return done();
          const timer = setTimeout(() => child.kill("SIGKILL"), KILL_AFTER_MS);
          child.once("exit", () => {
            clearTimeout(timer);
            done();
          });
          child.kill("SIGTERM");
        }),
    ),
  );
  children.length = 0;
}
