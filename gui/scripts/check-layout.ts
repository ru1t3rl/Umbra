// ADR 0046: one component or hook per file, each in its own directory so it is easy to find.
//   component  -> <pkg>/src/components/<Name>/<Name>.tsx   (CSS and tests beside it)
//   hook       -> <pkg>/src/hooks/<useName>.ts(x)          (test beside it)
//   context    -> <pkg>/src/context/<Name>Context.ts       (neither component nor hook)
// <pkg>/src is `src/` (the app), `features/<x>/src` or `feature-shells/<x>/src`.
// Exempt: tests (`*.test.*`, `*.test-d.*`), `generated/`, `reference/`, and `testing.ts(x)` test doubles (ASM-375).
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { extname, join, relative, sep } from "node:path";
import ts from "typescript";

export type LayoutRule =
  | "component-location"
  | "one-component-per-file"
  | "hook-location"
  | "one-hook-per-file"
  | "context-location"
  | "nested-declaration"
  | "file-name";

export interface LayoutViolation {
  file: string;
  rule: LayoutRule;
  message: string;
}

interface Declared {
  kind: "component" | "hook" | "context";
  name: string;
}

const SKIP_DIRS = new Set(["node_modules", "generated", "reference", ".tsbuild", "arch-fixtures"]);
const SCRIPT_KINDS: Record<string, ts.ScriptKind> = {
  ".tsx": ts.ScriptKind.TSX,
  ".jsx": ts.ScriptKind.JSX,
  ".js": ts.ScriptKind.JS,
};
const HOOK = /^use[A-Z]/;
const PASCAL = /^[A-Z][A-Za-z0-9]*$/;

function isExempt(name: string): boolean {
  return /\.test(-d)?\.[jt]sx?$/.test(name) || /^testing\.[jt]sx?$/.test(name);
}

function walk(dir: string, out: string[]): void {
  for (const entry of readdirSync(dir)) {
    if (SKIP_DIRS.has(entry)) continue;
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) walk(full, out);
    else if (/\.(tsx?|jsx?)$/.test(entry) && !isExempt(entry)) out.push(full);
  }
}

/** Every package `src` directory below the gui root: the app, features and shells. */
export function packageSrcDirs(root: string): string[] {
  const dirs = [join(root, "src")];
  for (const group of ["features", "feature-shells"]) {
    const groupDir = join(root, group);
    if (!existsSync(groupDir)) continue;
    for (const name of readdirSync(groupDir).sort()) dirs.push(join(groupDir, name, "src"));
  }
  return dirs.filter((d) => existsSync(d));
}

const CREATE_ELEMENT = new Set(["createElement", "jsx", "jsxs", "jsxDEV"]);
const WRAPPERS = new Set(["memo", "forwardRef", "lazy"]);

function calleeName(call: ts.CallExpression): string {
  const e = call.expression;
  if (ts.isIdentifier(e)) return e.text;
  if (ts.isPropertyAccessExpression(e)) return e.name.text;
  return "";
}

/** JSX syntax, or a `createElement`/`jsx` call (not `document.createElement`). */
function isJsxNode(node: ts.Node): boolean {
  if (ts.isJsxElement(node) || ts.isJsxSelfClosingElement(node) || ts.isJsxFragment(node)) {
    return true;
  }
  if (!ts.isCallExpression(node)) return false;
  const e = node.expression;
  if (ts.isIdentifier(e)) return CREATE_ELEMENT.has(e.text);
  return (
    ts.isPropertyAccessExpression(e) &&
    ts.isIdentifier(e.expression) &&
    e.expression.text === "React" &&
    CREATE_ELEMENT.has(e.name.text)
  );
}

function containsJsx(node: ts.Node): boolean {
  return isJsxNode(node) || ts.forEachChild(node, containsJsx) === true;
}

function unwrap(node: ts.Node): ts.Node {
  let n = node;
  while (
    ts.isParenthesizedExpression(n) ||
    ts.isAsExpression(n) ||
    ts.isSatisfiesExpression(n) ||
    ts.isNonNullExpression(n)
  ) {
    n = n.expression;
  }
  return n;
}

function isWrapperCall(n: ts.Node): boolean {
  if (!ts.isCallExpression(n)) return false;
  const callee = calleeName(n);
  return WRAPPERS.has(callee) || /^with[A-Z]/.test(callee);
}

function isFunctionLike(n: ts.Node): boolean {
  return (
    ts.isArrowFunction(n) ||
    ts.isFunctionExpression(n) ||
    ts.isFunctionDeclaration(n) ||
    ts.isMethodDeclaration(n)
  );
}

function isClassLike(n: ts.Node): boolean {
  return ts.isClassDeclaration(n) || ts.isClassExpression(n);
}

type Kind = Declared["kind"];

/** What a named value is: a context, a hook, a component, or nothing relevant. */
function classify(name: string, value: ts.Node): Kind | undefined {
  const v = unwrap(value);
  if (ts.isCallExpression(v) && calleeName(v) === "createContext") return "context";
  if (HOOK.test(name)) return isFunctionLike(v) || ts.isCallExpression(v) ? "hook" : undefined;
  if (PASCAL.test(name) || name === "default") {
    if (isWrapperCall(v)) return "component";
    if ((isFunctionLike(v) || isClassLike(v)) && containsJsx(v)) return "component";
  }
  return undefined;
}

function propertyName(n: ts.PropertyName): string | undefined {
  return ts.isIdentifier(n) || ts.isStringLiteral(n) ? n.text : undefined;
}

/** Name and value of a node that declares something, if it does. */
function declaredBy(node: ts.Node): { name: string; value: ts.Node } | undefined {
  if (ts.isFunctionDeclaration(node) || ts.isClassDeclaration(node)) {
    const isDefault = node.modifiers?.some((m) => m.kind === ts.SyntaxKind.DefaultKeyword);
    if (node.name) return { name: node.name.text, value: node };
    return isDefault ? { name: "default", value: node } : undefined;
  }
  if (ts.isVariableDeclaration(node) && ts.isIdentifier(node.name) && node.initializer) {
    return { name: node.name.text, value: node.initializer };
  }
  if (ts.isPropertyAssignment(node)) {
    const name = propertyName(node.name);
    return name === undefined ? undefined : { name, value: node.initializer };
  }
  if (ts.isMethodDeclaration(node)) {
    const name = propertyName(node.name);
    return name === undefined ? undefined : { name, value: node };
  }
  if (ts.isExportAssignment(node) && !node.isExportEquals) {
    return { name: "default", value: node.expression };
  }
  return undefined;
}

interface Found {
  declared: Declared[];
  nested: { kind: Kind; name: string }[];
}

/** A declaration is top level when its statement is a direct child of the source file. */
function isTopLevel(node: ts.Node, file: ts.SourceFile): boolean {
  if (ts.isVariableDeclaration(node)) {
    const statement = node.parent.parent;
    return ts.isVariableStatement(statement) && statement.parent === file;
  }
  return node.parent === file;
}

/** Every component, hook and context a file defines, top level or nested. */
function declarations(file: ts.SourceFile): Found {
  const found: Found = { declared: [], nested: [] };
  const visit = (node: ts.Node): void => {
    const d = declaredBy(node);
    const kind = d ? classify(d.name, d.value) : undefined;
    if (d && kind) {
      if (isTopLevel(node, file)) found.declared.push({ kind, name: d.name });
      else found.nested.push({ kind, name: d.name });
    }
    ts.forEachChild(node, visit);
  };
  visit(file);
  return found;
}

function checkFile(srcDir: string, path: string, rootDir: string): LayoutViolation[] {
  const out: LayoutViolation[] = [];
  const file = relative(rootDir, path).split(sep).join("/");
  const rel = relative(srcDir, path).split(sep).join("/");
  const text = readFileSync(path, "utf8");
  const source = ts.createSourceFile(
    path,
    text,
    ts.ScriptTarget.ES2022,
    true,
    SCRIPT_KINDS[extname(path)] ?? ts.ScriptKind.TS,
  );
  const { declared, nested } = declarations(source);
  const add = (rule: LayoutRule, message: string): void => {
    out.push({ file, rule, message });
  };

  const components = declared.filter((d) => d.kind === "component");
  const hooks = declared.filter((d) => d.kind === "hook");
  const contexts = declared.filter((d) => d.kind === "context");

  for (const n of nested) {
    add(
      "nested-declaration",
      `${n.kind} ${n.name} is declared inside another declaration; move it to its own file`,
    );
  }

  for (const c of components) {
    if (!rel.endsWith(`components/${c.name}/${c.name}.tsx`) || rel.split("/").length !== 3) {
      add(
        "component-location",
        c.name === "default"
          ? "anonymous default-exported component: give it a name and put it in components/<Name>/<Name>.tsx"
          : `component ${c.name} must live in components/${c.name}/${c.name}.tsx`,
      );
    }
  }
  if (components.length > 1) {
    add(
      "one-component-per-file",
      `defines ${components.length} components: ${components.map((c) => c.name).join(", ")}`,
    );
  }

  for (const h of hooks) {
    if (rel !== `hooks/${h.name}.ts` && rel !== `hooks/${h.name}.tsx`) {
      add("hook-location", `hook ${h.name} must live in hooks/${h.name}.ts(x)`);
    }
  }
  if (hooks.length > 1) {
    add(
      "one-hook-per-file",
      `defines ${hooks.length} hooks: ${hooks.map((h) => h.name).join(", ")}`,
    );
  }

  for (const c of contexts) {
    if (rel !== `context/${c.name}.ts` || !c.name.endsWith("Context")) {
      add(
        "context-location",
        `context ${c.name} must live in context/${c.name}.ts and end in "Context"`,
      );
    }
  }
  if (contexts.length > 1) {
    add("context-location", `defines ${contexts.length} contexts, one per file`);
  }

  // Files inside the three directories must carry the name of what they define.
  const parts = rel.split("/");
  if (parts[0] === "components") {
    const dir = parts[1] ?? "";
    const ok =
      parts.length === 3 && parts[2] === `${dir}.tsx` && components.some((c) => c.name === dir);
    if (!ok)
      add("file-name", `components/ files must be <Name>/<Name>.tsx defining <Name>, got ${rel}`);
  } else if (parts[0] === "hooks") {
    const base = (parts[1] ?? "").replace(/\.tsx?$/, "");
    const ok = parts.length === 2 && HOOK.test(base) && hooks.some((h) => h.name === base);
    if (!ok)
      add("file-name", `hooks/ files must be use<Name>.ts(x) defining use<Name>, got ${rel}`);
  } else if (parts[0] === "context") {
    const base = (parts[1] ?? "").replace(/\.ts$/, "");
    const ok = parts.length === 2 && contexts.some((c) => c.name === base);
    if (!ok)
      add(
        "file-name",
        `context/ files must be <Name>Context.ts defining <Name>Context, got ${rel}`,
      );
  }
  return out;
}

/** All layout violations below a gui root (the real tree, or a fixture with the same shape). */
export function findLayoutViolations(root: string): LayoutViolation[] {
  const out: LayoutViolation[] = [];
  for (const srcDir of packageSrcDirs(root)) {
    const files: string[] = [];
    walk(srcDir, files);
    for (const path of files.sort()) out.push(...checkFile(srcDir, path, root));
  }
  return out;
}

if (import.meta.main) {
  const root = process.argv[2] ?? join(import.meta.dir, "..");
  const violations = findLayoutViolations(root);
  for (const v of violations) console.error(`${v.file}: [${v.rule}] ${v.message}`);
  if (violations.length > 0) {
    console.error(`check:layout failed: ${violations.length} violation(s) (ADR 0046)`);
    process.exit(1);
  }
  console.log("check:layout: ok");
}
