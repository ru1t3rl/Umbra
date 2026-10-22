import { stopChildren } from "./support/children.ts";

/** SIGTERM, then SIGKILL after 5 s, for the engine and both preview servers. */
export default async function globalTeardown(): Promise<void> {
  await stopChildren();
}
