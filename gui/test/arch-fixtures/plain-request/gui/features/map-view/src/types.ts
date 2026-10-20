export type Other = string;
import type { Health }  from "@umbra/data"
import { useHealth, getHealthQueryKey, ApiProblem } from "@umbra/data";

export type { Health };
export const ok = [useHealth, getHealthQueryKey, ApiProblem];
