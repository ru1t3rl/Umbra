import { afterEach, beforeEach, expect, test } from "bun:test";
import { render, screen, waitFor } from "@testing-library/react";
import type { ReactElement } from "react";
import { DataProvider, shouldRetry } from "./DataProvider.tsx";
import { configureEngineUrl } from "./fetcher.ts";
import { useGetStatus } from "./generated/api.ts";
import { ApiProblem } from "./problem.ts";

const realFetch = globalThis.fetch;
beforeEach(() => configureEngineUrl("http://engine:7878"));
afterEach(() => {
  globalThis.fetch = realFetch;
  configureEngineUrl("");
});

function Probe(): ReactElement {
  const q = useGetStatus();
  if (q.isError) return <p role="alert">{`${q.error.code}:${q.error.status}`}</p>;
  if (q.data) return <p>{`api ${q.data.api_version}`}</p>;
  return <p>loading</p>;
}

test("a generated hook resolves through the provider, the configured engine URL and a fake fetch", async () => {
  const urls: string[] = [];
  globalThis.fetch = ((url: string) => {
    urls.push(url);
    return Promise.resolve(
      new Response(JSON.stringify({ api_version: "1.0.0" }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
  }) as unknown as typeof fetch;
  render(
    <DataProvider>
      <Probe />
    </DataProvider>,
  );
  expect(await screen.findByText("api 1.0.0")).toBeTruthy();
  expect(urls).toEqual(["http://engine:7878/api/v1/status"]);
});

test("a problem response reaches the hook as a typed ApiProblem, without a retry", async () => {
  let n = 0;
  globalThis.fetch = (() => {
    n += 1;
    return Promise.resolve(
      new Response(
        JSON.stringify({ code: "host_not_allowed", status: 403, title: "Forbidden", detail: "d" }),
        { status: 403, headers: { "content-type": "application/problem+json" } },
      ),
    );
  }) as unknown as typeof fetch;
  render(
    <DataProvider>
      <Probe />
    </DataProvider>,
  );
  await waitFor(() => expect(screen.getByRole("alert").textContent).toBe("host_not_allowed:403"));
  expect(n).toBe(1);
});

test("shouldRetry: never for a 4xx answer, up to two times otherwise", () => {
  const p = (status: number) => new ApiProblem({ code: "c", status, title: "t", detail: "d" });
  expect(shouldRetry(0, p(409))).toBe(false);
  expect(shouldRetry(0, p(503))).toBe(true);
  expect(shouldRetry(1, p(0))).toBe(true);
  expect(shouldRetry(2, p(503))).toBe(false);
});
