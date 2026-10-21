import { createContext } from "react";
import { browserTimers, type Timers } from "./model/announce.ts";

/** Internal test seam (not exported from the package): tests provide manual timers for the count announcements. */
export const AnnounceTimersContext = createContext<Timers<unknown>>(browserTimers);
