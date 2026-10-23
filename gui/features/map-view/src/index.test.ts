// WI-9 public API pin, by source text: importing ./index.ts would load the real maplibre-gl before the
// mocks of other test files (see the DashboardPage.test.tsx header). Catches: a runtime export leaking.
// `LiveMapHandle`, `LiveMapProps` and `MapInsets` are types and have no runtime key.
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

test("the package exports the LiveMap component only at runtime", () => {
  const source = readFileSync(new URL("./index.ts", import.meta.url), "utf8")
    .replace(/\/\/.*$/gm, "")
    .replace(/export\s+type\s*\{[^}]*\}\s*from\s*["'][^"']*["'];?/g, "");
  const runtime = [...source.matchAll(/export\s*(?:\*|\{([^}]*)\})[^;]*;/g)].flatMap((m) =>
    m[1] === undefined
      ? ["*"]
      : m[1].split(",").map(
          (n) =>
            n
              .trim()
              .split(/\s+as\s+/)
              .pop() ?? "",
        ),
  );
  expect(source).not.toMatch(/export\s+(const|function|class|let|var|default)\b/);
  expect(runtime.filter(Boolean).sort()).toEqual(["LiveMap"]);
});
