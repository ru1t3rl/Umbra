// WI-12 public API pin, by source text (importing ./index.ts would pull in every component; never import a
// feature index in a test that can reach real third-party modules, the WI-9 lesson). Catches: a runtime export
// leaking (SpawnForm, model helpers, test doubles), a public name missing. Types have no runtime key.
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("./index.ts", import.meta.url), "utf8").replace(
  /\/\/.*$/gm,
  "",
);

test("the package exports exactly TemplateList and TemplateDetail at runtime", () => {
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
  expect(runtime.filter(Boolean).sort()).toEqual(["TemplateDetail", "TemplateList"]);
});

test("the prop types are public", () => {
  expect(source).toMatch(/export\s+type\s*\{[^}]*\bTemplateListProps\b[^}]*\}/);
  expect(source).toMatch(/export\s+type\s*\{[^}]*\bTemplateDetailProps\b[^}]*\}/);
});

test("exports come from the component files, not from deep or foreign paths", () => {
  expect(source).toContain("./components/TemplateList/TemplateList.tsx");
  expect(source).toContain("./components/TemplateDetail/TemplateDetail.tsx");
  expect(source).not.toMatch(/from\s*["']@umbra\//);
});
