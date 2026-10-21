// orval mutator (F-5): every generated request goes through `engineFetch`. It prefixes the engine URL
// configured at runtime, sends JSON, returns the parsed body (204 -> undefined), and rejects with a typed
// `ApiProblem` for every non-2xx answer and for network failures.
import { ApiProblem, CLIENT_PROBLEM_CODES, internalProblem, problemFromBody } from "./problem.ts";

let engineUrl = "";

/** Set the engine base URL; "" means the page origin. Idempotent. `DataProvider` calls it; so can scripts. */
export function configureEngineUrl(url: string): void {
  engineUrl = url.replace(/\/+$/, "");
}

/** Join the configured engine URL and a request path (`/api/v1/...`). */
export function resolveUrl(path: string, base: string = engineUrl): string {
  return base === "" ? path : `${base}${path.startsWith("/") ? "" : "/"}${path}`;
}

/** orval uses these to type the hooks' error and body generics (`TError`). */
export type ErrorType<_Error> = ApiProblem;
export type BodyType<Body> = Body;

const isJson = (contentType: string | null): boolean =>
  contentType !== null && /(^|[/+;\s])json($|[;\s])/i.test(contentType);

async function readBody(response: Response): Promise<unknown> {
  const text = await response.text();
  if (text === "") return undefined;
  if (!isJson(response.headers.get("content-type"))) return text;
  return JSON.parse(text) as unknown;
}

export async function engineFetch<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers);
  if (typeof init.body === "string" && !headers.has("content-type")) {
    headers.set("content-type", "application/json");
  }
  if (!headers.has("accept")) headers.set("accept", "application/json, application/problem+json");

  let response: Response;
  try {
    response = await fetch(resolveUrl(path), { ...init, headers });
  } catch (error) {
    // A caller-initiated abort (TanStack Query cancellation) is not a failure of the engine.
    if (error instanceof DOMException && error.name === "AbortError") throw error;
    throw new ApiProblem({
      code: CLIENT_PROBLEM_CODES.networkError,
      status: 0,
      title: "Network error",
      detail: error instanceof Error ? error.message : String(error),
    });
  }

  let body: unknown;
  try {
    body = await readBody(response);
  } catch {
    throw internalProblem(response.status);
  }

  if (response.ok) return body as T;
  if (/application\/problem\+json/i.test(response.headers.get("content-type") ?? "")) {
    throw problemFromBody(body, response.status);
  }
  throw internalProblem(response.status);
}
