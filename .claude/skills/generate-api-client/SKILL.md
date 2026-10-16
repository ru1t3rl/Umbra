---
name: generate-api-client
description: Regenerate the TypeScript API client and stream types in gui/features/engine-client from api/openapi.json and the stream schema, with Bun. Use after any change to the Rust control API contract or when the drift check fails.
---

# Generate the API client

Run from `gui/`. Contract sources: `../api/openapi.json` and the stream JSON Schema produced by `cargo xtask gen-api` (see `update-openapi`).

1. Make sure the Rust side is regenerated and committed first (`cargo xtask gen-api`). Never edit the contract to suit the frontend; ask `api-engineer`.
2. `bun run gen:api` performs:
   - `bunx openapi-typescript ../api/openapi.json -o features/engine-client/src/generated/api.d.ts`
   - stream types from the stream schema into `features/engine-client/src/generated/stream.d.ts` (`bunx json-schema-to-typescript`)
   - a `// GENERATED, DO NOT EDIT` header on each file
3. `bun run gen:api --check` regenerates into a temp dir and fails on any difference. CI runs this.
4. Fix compile errors caused by regeneration by updating call sites and mappers in `engine-client`/`engine-stream`. Never silence them with `any` or `as unknown as`.
5. Wrap generated operations in small typed functions/hooks (TanStack Query) inside `engine-client`; features consume these wrappers, never `openapi-fetch` directly.
6. Run `verify-frontend`. Call out breaking contract changes in the PR so shells and features can be updated.
