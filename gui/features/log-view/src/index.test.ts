// WI-11 public API pin, by source text (the package imports the journal and, later, @umbra/desktop; the WI-9
// lesson is to avoid index imports where a mock could be bypassed). Catches: a runtime export leaking (model
// helpers, test doubles), a public name missing. Types have no runtime key.
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("./index.ts", import.meta.url), "utf8").replace(
  /\/\/.*$/gm,
  "",
);

test("the package exports exactly LogView at runtime", () => {
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
  expect(runtime.filter(Boolean).sort()).toEqual(["LogView"]);
});

test("the prop type is public", () => {
  expect(source).toMatch(/export\s+type\s*\{[^}]*\bLogViewProps\b[^}]*\}/);
});
