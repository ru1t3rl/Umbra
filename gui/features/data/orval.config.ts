// orval configuration for @umbra/data (F-5). Run through `bun run gen:api`, which passes the output
// directory via ORVAL_OUT (default: src/generated) so `--check` can generate into a temp dir.
import { defineConfig } from "orval";

const out = process.env.ORVAL_OUT ?? "src/generated";

export default defineConfig({
  umbra: {
    input: { target: process.env.ORVAL_INPUT ?? "../../../api/openapi.json" },
    output: {
      target: `${out}/api.ts`,
      mode: "single",
      client: "react-query",
      httpClient: "fetch",
      clean: true,
      prettier: false,
      override: {
        mutator: { path: "./src/fetcher.ts", name: "engineFetch" },
        fetch: { includeHttpResponseReturnType: false },
      },
    },
  },
});
