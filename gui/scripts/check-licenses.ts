// Fails on an installed npm package whose licence is outside the allow list (ADR 0043, NFR-4).
// Run: bun run check:licenses            Test fixtures: --root DIR (a tree with package.json + node_modules)
// Runtime closure = what ships (dependencies of the app and the workspace packages, transitively).
// Dev closure = everything else reachable from devDependencies; it may also use the dev-only licences.
import { existsSync, readdirSync, readFileSync, realpathSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

interface Pkg {
  name: string;
  version?: string;
  license?: string | { type?: string };
  licenses?: Array<{ type?: string } | string>;
  dependencies?: Record<string, string>;
  devDependencies?: Record<string, string>;
  optionalDependencies?: Record<string, string>;
  workspaces?: string[];
}
type Check = (allowed: string[]) => boolean;

const RUNTIME = ["MIT", "ISC", "BSD-2-Clause", "BSD-3-Clause", "Apache-2.0", "0BSD"];
const FONT_ONLY_RUNTIME = ["OFL-1.1"]; // font files only: allowed at runtime for @fontsource/* packages
const DEV = [
  ...RUNTIME,
  ...FONT_ONLY_RUNTIME,
  "MPL-2.0",
  "CC-BY-4.0",
  "Python-2.0",
  "BlueOak-1.0.0",
  "CC0-1.0",
];

const argRoot = process.argv.indexOf("--root");
if (argRoot > 0 && !process.argv[argRoot + 1]) {
  console.error("usage: check-licenses.ts [--root DIR]");
  process.exit(2);
}
const root = resolve(
  argRoot > 0 ? (process.argv[argRoot + 1] as string) : join(import.meta.dir, ".."),
);
const readJson = (p: string): Pkg => JSON.parse(readFileSync(p, "utf8")) as Pkg;

// Workspace members (patterns of the form `dir/*` or `dir`).
const rootPkg = readJson(join(root, "package.json"));
const members: Array<{ dir: string; pkg: Pkg }> = [{ dir: root, pkg: rootPkg }];
for (const pattern of rootPkg.workspaces ?? []) {
  const parent = join(root, pattern.replace(/\/\*$/, ""));
  const dirs = pattern.endsWith("/*")
    ? existsSync(parent)
      ? readdirSync(parent).map((n) => join(parent, n))
      : []
    : [parent];
  for (const dir of dirs) {
    if (existsSync(join(dir, "package.json")))
      members.push({ dir, pkg: readJson(join(dir, "package.json")) });
  }
}
const memberNames = new Set(members.map((m) => m.pkg.name));

const find = (name: string, from: string): string | null => {
  for (let dir = from; ; dir = dirname(dir)) {
    const p = join(dir, "node_modules", name, "package.json");
    if (existsSync(p)) return realpathSync(dirname(p));
    if (dir === root || dir === dirname(dir)) return null;
  }
};

const problems: string[] = [];
interface Seed {
  name: string;
  from: string;
  optional: boolean;
}
const walkClosure = (seeds: Seed[], seen: Map<string, Pkg>): void => {
  const stack = [...seeds];
  for (let s = stack.pop(); s; s = stack.pop()) {
    if (memberNames.has(s.name)) continue;
    const dir = find(s.name, s.from);
    if (!dir) {
      if (!s.optional) problems.push(`${s.name}: declared but not installed (run bun install)`);
      continue;
    }
    if (seen.has(dir)) continue;
    const pkg = readJson(join(dir, "package.json"));
    seen.set(dir, pkg);
    for (const n of Object.keys(pkg.dependencies ?? {}))
      stack.push({ name: n, from: dir, optional: false });
    for (const n of Object.keys(pkg.optionalDependencies ?? {}))
      stack.push({ name: n, from: dir, optional: true });
  }
};

const seedsOf = (pick: (p: Pkg) => Record<string, string> | undefined): Seed[] =>
  members.flatMap((m) =>
    Object.keys(pick(m.pkg) ?? {}).map((name) => ({ name, from: m.dir, optional: false })),
  );
const runtime = new Map<string, Pkg>();
const dev = new Map<string, Pkg>();
walkClosure(
  seedsOf((p) => p.dependencies),
  runtime,
);
walkClosure(
  seedsOf((p) => p.devDependencies),
  dev,
);
for (const dir of runtime.keys()) dev.delete(dir);

// SPDX expression: `A OR B`, `A AND B`, parentheses; OR binds weaker than AND. Leftover tokens are an error.
const parse = (expr: string): Check | null => {
  const tokens = expr.match(/\(|\)|[^\s()]+/g) ?? [];
  let i = 0;
  const atom = (): Check => {
    const t = tokens[i++];
    if (t === "(") {
      const v = or();
      i++;
      return v;
    }
    return (allowed) => t !== undefined && allowed.includes(t.replace(/\+$/, ""));
  };
  const and = (): Check => {
    let l = atom();
    while (tokens[i] === "AND") {
      i++;
      const a = l;
      const b = atom();
      l = (x) => a(x) && b(x);
    }
    return l;
  };
  const or = (): Check => {
    let l = and();
    while (tokens[i] === "OR") {
      i++;
      const a = l;
      const b = and();
      l = (x) => a(x) || b(x);
    }
    return l;
  };
  const result = or();
  return i === tokens.length ? result : null;
};
const licenceOf = (pkg: Pkg): string | null => {
  if (typeof pkg.license === "string") return pkg.license;
  if (pkg.license?.type) return pkg.license.type;
  if (Array.isArray(pkg.licenses))
    return `(${pkg.licenses.map((l) => (typeof l === "string" ? l : l.type)).join(" OR ")})`;
  return null;
};
const judge = (map: Map<string, Pkg>, kind: "runtime" | "dev"): void => {
  for (const pkg of map.values()) {
    const lic = licenceOf(pkg);
    const label = `${pkg.name}@${pkg.version}`;
    if (!lic) {
      problems.push(`${label}: no licence field (${kind})`);
      continue;
    }
    const fontRuntime = kind === "runtime" && pkg.name.startsWith("@fontsource/");
    const allowed =
      kind === "dev" ? DEV : fontRuntime ? [...RUNTIME, ...FONT_ONLY_RUNTIME] : RUNTIME;
    const check = parse(lic);
    if (!check) problems.push(`${label}: unparsable licence expression "${lic}" (${kind})`);
    else if (!check(allowed)) problems.push(`${label}: ${lic} is not allowed (${kind})`);
  }
};
judge(runtime, "runtime");
judge(dev, "dev");

for (const p of problems.sort()) console.error(p);
console.log(
  problems.length
    ? `\n${problems.length} licence problem(s)`
    : `licences OK (${runtime.size} runtime, ${dev.size} dev-only packages)`,
);
process.exit(problems.length ? 1 : 0);
