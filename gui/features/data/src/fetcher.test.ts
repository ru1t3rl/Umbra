import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { configureEngineUrl, engineFetch, resolveUrl } from "./fetcher.ts";
import { type ApiProblem, isApiProblem } from "./problem.ts";

interface Call {
  url: string;
  init: RequestInit;
}
const realFetch = globalThis.fetch;
let calls: Call[] = [];

function fakeFetch(respond: () => Response | Promise<Response>): void {
  globalThis.fetch = ((url: string, init: RequestInit) => {
    calls.push({ url, init });
    return Promise.resolve(respond());
  }) as unknown as typeof fetch;
}
const json = (body: unknown, status = 200, type = "application/json"): Response =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": type } });

async function rejection(p: Promise<unknown>): Promise<ApiProblem> {
  try {
    await p;
  } catch (e) {
    if (isApiProblem(e)) return e;
    throw e;
  }
  throw new Error("expected the request to reject with an ApiProblem");
}

beforeEach(() => {
  calls = [];
  configureEngineUrl("");
});
afterEach(() => {
  globalThis.fetch = realFetch;
  configureEngineUrl("");
});

describe("base URL", () => {
  test("an empty engine URL keeps the path (same origin)", async () => {
    fakeFetch(() => json({ ok: 1 }));
    await engineFetch("/api/v1/status");
    expect(calls[0]?.url).toBe("/api/v1/status");
  });
  test("a configured URL is prefixed, with one slash between", async () => {
    configureEngineUrl("http://engine:7878/");
    fakeFetch(() => json({ ok: 1 }));
    await engineFetch("/api/v1/status");
    expect(calls[0]?.url).toBe("http://engine:7878/api/v1/status");
  });
  test("resolveUrl adds the missing slash", () => {
    expect(resolveUrl("api/v1/x", "http://h")).toBe("http://h/api/v1/x");
  });
});

describe("requests", () => {
  test("a string body gets a JSON content-type; a caller's content-type wins", async () => {
    fakeFetch(() => json({}));
    await engineFetch("/x", { method: "POST", body: "{}" });
    await engineFetch("/x", { method: "POST", body: "{}", headers: { "content-type": "text/x" } });
    expect(new Headers(calls[0]?.init.headers).get("content-type")).toBe("application/json");
    expect(new Headers(calls[1]?.init.headers).get("content-type")).toBe("text/x");
  });
  test("a request without a body sets no content-type", async () => {
    fakeFetch(() => json({}));
    await engineFetch("/x");
    expect(new Headers(calls[0]?.init.headers).has("content-type")).toBe(false);
  });
});

describe("responses", () => {
  test("a JSON body is parsed", async () => {
    fakeFetch(() => json({ tick: 7 }));
    expect(await engineFetch("/x")).toEqual({ tick: 7 });
  });
  test("204 resolves to undefined", async () => {
    fakeFetch(() => new Response(null, { status: 204 }));
    expect(await engineFetch("/x", { method: "DELETE" })).toBeUndefined();
  });
  test("application/problem+json becomes an ApiProblem with all fields", async () => {
    const diagnostics = [{ message: "bad" }];
    fakeFetch(() =>
      json(
        {
          code: "scenario_invalid",
          status: 422,
          title: "Invalid",
          detail: "see list",
          diagnostics,
        },
        422,
        "application/problem+json",
      ),
    );
    const p = await rejection(engineFetch("/x"));
    expect(p.code).toBe("scenario_invalid");
    expect(p.status).toBe(422);
    expect(p.title).toBe("Invalid");
    expect(p.detail).toBe("see list");
    expect(p.diagnostics).toEqual(diagnostics);
  });
  test("a null diagnostics field is omitted", async () => {
    fakeFetch(() =>
      json(
        { code: "no_scenario", status: 409, title: "t", detail: "d", diagnostics: null },
        409,
        "application/problem+json",
      ),
    );
    const p = await rejection(engineFetch("/x"));
    expect(p.code).toBe("no_scenario");
    expect(p.diagnostics).toBeUndefined();
  });
  test("a non-problem error body becomes code internal with the HTTP status", async () => {
    fakeFetch(
      () =>
        new Response("<html>bad gateway</html>", {
          status: 502,
          headers: { "content-type": "text/html" },
        }),
    );
    const p = await rejection(engineFetch("/x"));
    expect(p.code).toBe("internal");
    expect(p.status).toBe(502);
  });
  test("a JSON error that is not a problem document is internal too", async () => {
    fakeFetch(() => json({ message: "nope" }, 500, "application/problem+json"));
    const p = await rejection(engineFetch("/x"));
    expect(p.code).toBe("internal");
    expect(p.status).toBe(500);
  });
  test("a success body that is not valid JSON is internal", async () => {
    fakeFetch(
      () => new Response("{oops", { status: 200, headers: { "content-type": "application/json" } }),
    );
    const p = await rejection(engineFetch("/x"));
    expect(p.code).toBe("internal");
    expect(p.status).toBe(200);
  });
  test("an abort is rethrown unchanged, not turned into network_error", async () => {
    const abort = new DOMException("aborted", "AbortError");
    globalThis.fetch = (() => Promise.reject(abort)) as unknown as typeof fetch;
    let caught: unknown;
    try {
      await engineFetch("/x");
    } catch (e) {
      caught = e;
    }
    expect(caught).toBe(abort);
    expect(isApiProblem(caught)).toBe(false);
  });
  test("a network failure becomes network_error with status 0", async () => {
    globalThis.fetch = (() =>
      Promise.reject(new TypeError("connection refused"))) as unknown as typeof fetch;
    const p = await rejection(engineFetch("/x"));
    expect(p.code).toBe("network_error");
    expect(p.status).toBe(0);
    expect(p.detail).toBe("connection refused");
  });
});
