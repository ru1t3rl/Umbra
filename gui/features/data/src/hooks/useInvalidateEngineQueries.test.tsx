import { afterEach, expect, test } from "bun:test";
import { QueryClient } from "@tanstack/react-query";
import { act, cleanup, renderHook } from "@testing-library/react";
import type { ReactElement, ReactNode } from "react";
import { DataProvider } from "../components/DataProvider/DataProvider.tsx";
import {
  getGetEntityQueryKey,
  getGetPluginsQueryKey,
  getGetStatusQueryKey,
  getGetTemplateQueryKey,
  getListEntitiesQueryKey,
  getListTemplatesQueryKey,
} from "../generated/api.ts";
import { useInvalidateEngineQueries } from "./useInvalidateEngineQueries.ts";

afterEach(cleanup);

function setup() {
  const client = new QueryClient();
  const wrapper = ({ children }: { children: ReactNode }): ReactElement => (
    <DataProvider client={client}>{children}</DataProvider>
  );
  return { client, wrapper };
}

const stale = (client: QueryClient, key: readonly unknown[]) =>
  client.getQueryState(key)?.isInvalidated;

test("invalidates every generated engine query key except status", async () => {
  const { client, wrapper } = setup();
  const engine = [
    getListEntitiesQueryKey(),
    getGetEntityQueryKey(7),
    getGetPluginsQueryKey(),
    getListTemplatesQueryKey(),
    getGetTemplateQueryKey("t1"),
  ];
  for (const key of engine) client.setQueryData(key, {});
  client.setQueryData(getGetStatusQueryKey(), {});
  const { result } = renderHook(() => useInvalidateEngineQueries(), { wrapper });
  await act(async () => {
    await result.current();
  });
  for (const key of engine) expect(stale(client, key)).toBe(true);
  expect(stale(client, getGetStatusQueryKey())).toBe(false);
});

test("returns a stable function across renders", () => {
  const { wrapper } = setup();
  const { result, rerender } = renderHook(() => useInvalidateEngineQueries(), { wrapper });
  const first = result.current;
  rerender();
  expect(result.current).toBe(first);
});
