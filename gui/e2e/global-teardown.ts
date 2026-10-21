import { childRegistry } from "./global-setup.ts";

const KILL_AFTER_MS = 5_000;

/** SIGTERM, then SIGKILL after 5 s. */
export default async function globalTeardown(): Promise<void> {
  const children = childRegistry();
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
