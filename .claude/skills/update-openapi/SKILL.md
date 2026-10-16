---
name: update-openapi
description: Add or change a REST endpoint or stream message in cgf-server/cgf-api-types and regenerate the committed OpenAPI and stream schemas. Use for any change to the HTTP/WebSocket contract.
---

# Update the API contract

Requirements: REQ-X-4..9, D-11, D-12. Owner: `api-engineer`.

1. **Wire types first** in `cgf-api-types`: DTOs with `serde`, `utoipa::ToSchema`, `schemars::JsonSchema` and TS export derives. Never expose `cgf-core` types directly; map at the edge.
2. **Handler** in `cgf-server`: thin, calls only `EngineHandle`, returns problem-details errors with stable codes. Annotate with `#[utoipa::path]` including all error responses.
3. **Versioning**: additive changes stay in `/api/v1`. A breaking change needs an ADR and a new `/api/v2`.
4. **Regenerate**: run `cargo xtask gen-api` which writes `api/openapi.json` and the stream JSON Schema/AsyncAPI file. Commit them.
5. **Drift check**: `cargo xtask gen-api --check` must pass (CI runs it).
6. **Tests**: handler test, contract test against the spec, error shape test; for streams a back-pressure test.
7. **Frontend**: hand off to `frontend-engineer` to run the `generate-api-client` skill (`bun run gen:api` in `gui/`); never hand-edit generated files.
8. Run `review-change` and `verify-and-build`.
