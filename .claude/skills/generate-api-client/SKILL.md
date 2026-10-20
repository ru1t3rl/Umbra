---
name: generate-api-client
description: Regenerate the orval REST client and TanStack Query hooks in gui/features/data (@umbra/data) from api/openapi.json, and the stream types in gui/features/engine-stream from api/stream.schema.json, with Bun. Use after any change to the Rust control API contract or when the drift check fails.
---

# Generate the API client

Run from `gui/`. Contract sources: `../api/openapi.json` and the stream JSON Schema produced by `cargo xtask gen-api` (see `update-openapi`).

1. Make sure the Rust side is regenerated and committed first (`cargo xtask gen-api`). Never edit the contract to suit the frontend; ask `api-engineer`.
2. `bun run gen:api` performs:
   - `orval --config features/data/orval.config.ts`: the fetch client and `useQuery`/`useMutation` hooks for every operation of `../api/openapi.json` into `features/data/src/generated/` (`client: 'react-query'`, `httpClient: 'fetch'`, mutator `features/data/src/fetcher.ts`). orval is a pinned dev dependency of `@umbra/data` (exact version in `bun.lock`), never run through an unpinned `bunx`
   - stream types from `../api/stream.schema.json` into `features/engine-stream/src/generated/stream.d.ts` (`json-schema-to-typescript`, pinned the same way)
   - a `// GENERATED, DO NOT EDIT` header on each file
3. `bun run gen:api --check` regenerates into a temp dir and fails on any difference. CI runs this.
4. Fix compile errors caused by regeneration by updating call sites and mappers in `data`/`engine-stream`. Never silence them with `any` or `as unknown as`.
5. `@umbra/data` re-exports the generated hooks and query keys from `src/index.ts`; features import them from `@umbra/data` only, never from `generated/` and never call `fetch` for REST. Errors surface as the typed `ApiProblem` (`code`, `status`) from the mutator.
   - First M5b use: verify that orval handles the committed OpenAPI 3.1 document (type arrays such as `["integer","null"]` as nullable, `application/problem+json` error types, the repeated `ids` array style). If it does not, stop and report; never change the server contract to suit the generator.
6. Run `verify-frontend`. Call out breaking contract changes in the PR so shells and features can be updated.
