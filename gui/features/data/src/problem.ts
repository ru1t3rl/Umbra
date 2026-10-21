// The typed error every failed REST call rejects with (F-5). It mirrors the contract's `Problem`
// (`application/problem+json`, docs/api.md) but its `code` is a plain string so that codes added in a
// later minor version, and the two client-side codes below, never need a type change.

/** Client-side codes (not part of the contract's `ProblemCode`). */
export const CLIENT_PROBLEM_CODES = {
  /** The response was not a problem document (or not JSON); `status` is the HTTP status. */
  internal: "internal",
  /** The request never produced a response (offline, refused, aborted); `status` is 0. */
  networkError: "network_error",
} as const;

export interface ApiProblemInit {
  code: string;
  status: number;
  title: string;
  detail: string;
  diagnostics?: readonly unknown[];
}

export class ApiProblem extends Error implements ApiProblemInit {
  readonly code: string;
  readonly status: number;
  readonly title: string;
  readonly detail: string;
  readonly diagnostics?: readonly unknown[];

  constructor(init: ApiProblemInit) {
    super(`${init.title}: ${init.detail}`);
    this.name = "ApiProblem";
    this.code = init.code;
    this.status = init.status;
    this.title = init.title;
    this.detail = init.detail;
    if (init.diagnostics !== undefined) this.diagnostics = init.diagnostics;
  }
}

export function isApiProblem(value: unknown): value is ApiProblem {
  return value instanceof ApiProblem;
}

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);

/** Decode an `application/problem+json` body; anything malformed becomes `internal` with the HTTP status. */
export function problemFromBody(body: unknown, status: number): ApiProblem {
  if (isRecord(body) && typeof body.code === "string" && typeof body.title === "string") {
    return new ApiProblem({
      code: body.code,
      status: typeof body.status === "number" ? body.status : status,
      title: body.title,
      detail: typeof body.detail === "string" ? body.detail : "",
      ...(Array.isArray(body.diagnostics) ? { diagnostics: body.diagnostics } : {}),
    });
  }
  return internalProblem(status);
}

export function internalProblem(status: number): ApiProblem {
  return new ApiProblem({
    code: CLIENT_PROBLEM_CODES.internal,
    status,
    title: "Unexpected response",
    detail: `The engine answered ${status} without a problem document.`,
  });
}
