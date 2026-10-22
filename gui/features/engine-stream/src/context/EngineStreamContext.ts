import { createContext } from "react";
import type { EngineStreamConnection } from "../connection.ts";

export const EngineStreamContext = createContext<EngineStreamConnection | undefined>(undefined);
