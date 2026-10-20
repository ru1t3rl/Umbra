---
name: update-openapi
description: Add or change a REST endpoint or stream message in umbra-server/umbra-api-types and regenerate the committed contract files in api/. Use for any change to the HTTP/WebSocket contract.
---

# Update the API contract

Requirements: REQ-X-4..9, D-11, D-12, ADR 0025, ADR 0026. Owner: `api-engineer`. The reference page is
`docs/api.md`; the contract is the three committed files `api/openapi.json`, `api/stream.schema.json` and
`api/asyncapi.json`. There is no CI (ADR 0004): gate 10 and the local tests are the check.

1. **Wire types first** in `umbra-api-types`: DTOs with `serde` and `utoipa::ToSchema` derives only. No
   `schemars`, no Rust-side TypeScript derive (ADR 0025): the stream JSON Schema is built from the same
   `utoipa` schemas, and TypeScript is generated in `gui/`. Never expose `umbra-core` or kernel types;
   map at the edge (`umbra-server/src/map`). A new DTO goes into the `ApiSchemas` list in
   `crates/umbra-api-types/src/lib.rs`; a new stream message goes into `StreamMessage` as well.
2. **Handler** in `umbra-server/src/handlers/`: thin, calls only `EngineHandle` (REQ-X-7), returns
   problem-details errors with stable codes. Annotate with `#[utoipa::path]` (`operation_id`, one `tag`, the
   success response, the route's problem responses, and the shared `CommonProblems`) and list the function
   in the `paths(...)` of `crates/umbra-server/src/openapi.rs`. Register the route in `routes.rs`; mutating
   routes go through the one mutating layer.
3. **Errors.** A new code is added to the `problem_codes!` table in `umbra-api-types/src/problem.rs` with its
   one HTTP status (additive, a minor bump). Use the custom `ApiJson`/`ApiQuery`/`ApiPath` extractors so a
   bare framework error never reaches a client.
4. **Versioning.** Additive changes stay in `/api/v1` and bump the minor of `API_VERSION` (new endpoint,
   optional field, response field, enum value, code, stream message type). A breaking change (removing or
   changing anything) needs an ADR and `/api/v2`. Within v1 never break the committed files.
5. **Regenerate**: `cargo xtask gen-api` writes `api/openapi.json`, `api/stream.schema.json` and
   `api/asyncapi.json` (it runs the `gen_api` example of `umbra-server`). Commit them with the change.
6. **Drift check (gate 10)**: `cargo xtask gen-api --check` must pass (exit 1 is drift). It is part of
   `scripts/gates.sh`. The test `served_openapi_equals_committed_file` fails too if the served document
   and the file differ. The old `NOT_YET` list of the contract test is gone: every route must be
   documented.
7. **Docs.** Update the tables of `docs/api.md` (endpoints with operation ids, error codes with statuses,
   run states, configuration keys). `crates/umbra-server/tests/docs_api.rs` (gate 3) fails when they differ
   from the contract, `ProblemCode::ALL` or `config.v1.schema.json`, in both directions.
8. **Tests**: handler test (`umbra-server/tests/rest_*.rs`), contract tests (`contract.rs`: routes,
   operation ids, response conformance with the closed-world walk), error-shape test (`problems.rs`), and
   for streams the protocol and back-pressure tests (`stream.rs`, `slow_client.rs`). Mutate the new guard
   and show that a test fails (`docs/implementer-checklist.md`).
9. **Frontend**: tell `frontend-engineer` to run the `generate-api-client` skill (`bun run gen:api` in
   `gui/`) after the commit; never hand-edit generated files.
10. Run `review-change`, then `verify-and-build` (`scripts/gates.sh`).
