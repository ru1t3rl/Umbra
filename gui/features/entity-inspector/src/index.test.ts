// WI-10 public API pin, by source text (importing ./index.ts would pull in every component; the WI-9 lesson
// is to avoid index imports in packages that can reach real third-party modules). Catches: a runtime export
// leaking (model helpers, test doubles), a public name missing. Types have no runtime key.
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("./index.ts", import.meta.url), "utf8").replace(
  /\/\/.*$/gm,
  "",
);

test("the package exports exactly EntityListPanel and EntityInspector at runtime", () => {
  const withoutTypes = source.replace(/export\s+type\s*\{[^}]*\}\s*from\s*["'][^"']*["'];?/g, "");
  const runtime = [...withoutTypes.matchAll(/export\s*(?:\*|\{([^}]*)\})[^;]*;/g)].flatMap((m) =>
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
  expect(withoutTypes).not.toMatch(/export\s+(const|function|class|let|var|default)\b/);
  expect(runtime.filter(Boolean).sort()).toEqual(["EntityInspector", "EntityListPanel"]);
});

test("the prop types are public", () => {
  expect(source).toMatch(/export\s+type\s*\{[^}]*\bEntityListPanelProps\b[^}]*\}/);
  expect(source).toMatch(/export\s+type\s*\{[^}]*\bEntityInspectorProps\b[^}]*\}/);
});
