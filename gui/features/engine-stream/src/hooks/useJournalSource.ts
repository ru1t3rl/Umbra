import type { JournalSource } from "../types.ts";
import { useConnection } from "./useConnection.ts";

/** Stable imperative source of journal records; it never re-renders the caller (ADR 0071). */
export function useJournalSource(): JournalSource {
  return useConnection().journal;
}
