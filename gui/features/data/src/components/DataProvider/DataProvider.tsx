import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { type ReactElement, type ReactNode, useState } from "react";
import { isApiProblem } from "../../problem.ts";

export interface DataProviderProps {
  children: ReactNode;
  /** Tests inject their own client; the app lets the provider create one. */
  client?: QueryClient;
}

/** Client errors (4xx) are answers, not faults: retrying them cannot change the result. */
export function shouldRetry(failureCount: number, error: unknown): boolean {
  if (isApiProblem(error) && error.status >= 400 && error.status < 500) return false;
  return failureCount < 2;
}

/**
 * Provides the TanStack Query client for the generated hooks. The engine URL is not here: the request
 * mutator is module-level, so the app bootstrap calls `configureEngineUrl` once after the config loads.
 */
export function DataProvider({ children, client }: DataProviderProps): ReactElement {
  const [own] = useState(
    () => client ?? new QueryClient({ defaultOptions: { queries: { retry: shouldRetry } } }),
  );
  return <QueryClientProvider client={own}>{children}</QueryClientProvider>;
}
