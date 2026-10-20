// ADR 0045: @umbra/data re-exports orval's plain request functions for e2e/ and scripts only.
// Features and shells may import hooks (use*), query-key/option getters, types and the data-layer helpers.
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

export interface PlainRequestImport {
  file: string;
  name: string;
}

const ALLOWED = [
  /^use[A-Z]/,
  /^get\w+Query(Key|Options)$/,
  /^[A-Z]/, // components, classes and types
  /^(configureEngineUrl|isApiProblem)$/,
];
const SPECIFIER = `["']@umbra/data["']`;
// import/export clauses are limited to braces, `* as x` or one identifier, so a semicolon-less
// `import type` cannot swallow the next statement.
const STATIC = new RegExp(
  `(import|export)\\s+(type\\s+)?(\\{[^}]*\\}|\\*(?:\\s+as\\s+\\w+)?|\\w+)(?:\\s*,\\s*(\\{[^}]*\\}))?\\s+from\\s+${SPECIFIER}`,
  "g",
);
const DYNAMIC = new RegExp(`\\bimport\\s*\\(\\s*${SPECIFIER}\\s*\\)`, "g");

function sourceFiles(dir: string): string[] {
  if (!existsSync(dir)) return [];
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    if (entry === "node_modules" || entry === "generated" || entry === "arch-fixtures") continue;
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) out.push(...sourceFiles(full));
    else if (/\.tsx?$/.test(entry)) out.push(full);
  }
  return out;
}

function namesOf(clause: string): string[] {
  if (!clause.startsWith("{")) return [clause]; // default, `*` or `* as x`: never allowed
  return clause
    .replace(/[{}]/g, "")
    .split(",")
    .map((n) => n.trim())
    .filter((n) => n !== "" && !n.startsWith("type "))
    .map((n) => (n.split(/\s+as\s+/)[0] ?? n).trim());
}

/**
 * Value imports/re-exports/dynamic imports of non-hook names from `@umbra/data` in features (except data
 * itself), shells, the app and tests. Only `e2e/` and `scripts/` may use plain request functions.
 */
export function findPlainRequestImports(root: string): PlainRequestImport[] {
  const featuresDir = join(root, "features");
  const files = [
    ...(existsSync(featuresDir)
      ? readdirSync(featuresDir, { withFileTypes: true })
          .filter((d) => d.isDirectory() && d.name !== "data")
          .flatMap((d) => sourceFiles(join(featuresDir, d.name)))
      : []),
    ...sourceFiles(join(root, "feature-shells")),
    ...sourceFiles(join(root, "src")),
    ...sourceFiles(join(root, "test")),
  ];
  const found: PlainRequestImport[] = [];
  for (const file of files) {
    const text = readFileSync(file, "utf8");
    const rel = relative(root, file);
    for (const m of text.matchAll(STATIC)) {
      if (m[2]) continue; // `import type` / `export type`
      for (const name of [...namesOf(m[3] ?? ""), ...namesOf(m[4] ?? "")]) {
        if (name !== "" && !ALLOWED.some((re) => re.test(name))) found.push({ file: rel, name });
      }
    }
    for (const _ of text.matchAll(DYNAMIC))
      found.push({ file: rel, name: 'import("@umbra/data")' });
  }
  return found.sort((a, b) => a.file.localeCompare(b.file) || a.name.localeCompare(b.name));
}

if (import.meta.main) {
  const bad = findPlainRequestImports(process.cwd());
  for (const b of bad)
    console.error(`${b.file}: plain request function "${b.name}" from @umbra/data (ADR 0045)`);
  if (bad.length > 0) process.exit(1);
  console.log("no plain request functions imported by features or shells");
}
