// WI-11 prop contract (compile-time; `tsc -b` checks it, ASM-944 e).
import type { LogViewProps } from "../index.ts";

export const full: LogViewProps = { label: "Log" };
export const compact: LogViewProps = { label: "Log", compact: true };
export const explicit: LogViewProps = { label: "Log", compact: false };
// @ts-expect-error label is required: the list needs an accessible name
export const noLabel: LogViewProps = { compact: true };
