// Generates tokens.css and tokens.ts from tokens.json (single source of truth).
// Run: bun run gen:tokens   |   Check for drift: bun run gen:tokens --check
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "features", "ui-kit", "src", "tokens");
const t = JSON.parse(readFileSync(join(root, "tokens.json"), "utf8"));
const check = process.argv.includes("--check");

const ref = (v) => (typeof v === "string" ? v.replace(/\{([\w-]+)\}/g, "var(--$1)") : v);
const themed = (v, theme) => ref(typeof v === "object" ? v[theme] : v);
const lines = (obj) => Object.entries(obj).map(([k, v]) => `  --${k}: ${v};`);

const shared = {};
for (const [k, v] of Object.entries(t.type.families)) shared[`font-${k}`] = v;
for (const s of t.spacing.tokens) shared[s.name] = s.value;
for (const s of t.radius.tokens) shared[s.name] = s.value;
for (const s of t.rowHeight.tokens) shared[s.name] = s.value;
for (const s of t.blur.tokens) shared[s.name] = s.value;
for (const s of t.opacity.tokens) shared[s.name] = s.value;
for (const s of t.duration.tokens) shared[s.name] = s.value;
for (const s of t.easing.tokens) shared[s.name] = s.value;

const themeVars = (theme) => {
  const out = {};
  for (const c of t.color.tokens) out[c.name] = themed(c.value, theme);
  for (const s of t.shadow.tokens) out[s.name] = themed(s.value, theme);
  return out;
};

const typeClasses = t.type.groups.flatMap((g) =>
  g.styles.map((s) => {
    const mono = g.family === "mono";
    const rules = [
      `font-family: var(--font-${g.family})`,
      `font-size: ${s.fontSize}`,
      `line-height: ${s.lineHeight}`,
      `font-weight: ${s.fontWeight}`,
      s.letterSpacing ? `letter-spacing: ${s.letterSpacing}` : null,
      mono ? "font-variant-numeric: tabular-nums" : null,
      s.name === "label" ? "text-transform: uppercase" : null,
    ].filter(Boolean);
    return `.type-${s.name} { ${rules.join("; ")}; }`;
  })
);

const css = [
  "/* GENERATED from tokens.json by gui/scripts/gen-tokens.mjs. Do not edit. */",
  `:root, :root[data-theme="dark"] {\n  color-scheme: dark;\n${lines({ ...shared, ...themeVars("dark") }).join("\n")}\n}`,
  `:root[data-theme="light"] {\n  color-scheme: light;\n${lines(themeVars("light")).join("\n")}\n}`,
  `@media (prefers-reduced-motion: reduce) {\n  :root, :root[data-theme] {\n${t.duration.tokens.map((d) => `    --${d.name}: 0ms;`).join("\n")}\n  }\n}`,
  typeClasses.join("\n"),
  "",
].join("\n\n");

const names = (arr) => JSON.stringify(arr.map((x) => x.name));
const ts = `// GENERATED from tokens.json by gui/scripts/gen-tokens.mjs. Do not edit.
export const colorTokens = ${names(t.color.tokens)} as const;
export const spacingTokens = ${names(t.spacing.tokens)} as const;
export const radiusTokens = ${names(t.radius.tokens)} as const;
export const rowHeightTokens = ${names(t.rowHeight.tokens)} as const;
export const shadowTokens = ${names(t.shadow.tokens)} as const;
export const blurTokens = ${names(t.blur.tokens)} as const;
export const opacityTokens = ${names(t.opacity.tokens)} as const;
export const durationTokens = ${names(t.duration.tokens)} as const;
export const easingTokens = ${names(t.easing.tokens)} as const;
export const typeStyles = ${JSON.stringify(t.type.groups.flatMap((g) => g.styles.map((s) => s.name)))} as const;
export type ColorToken = (typeof colorTokens)[number];
export type SpacingToken = (typeof spacingTokens)[number];
export type RadiusToken = (typeof radiusTokens)[number];
export type RowHeightToken = (typeof rowHeightTokens)[number];
export type ShadowToken = (typeof shadowTokens)[number];
export type BlurToken = (typeof blurTokens)[number];
export type OpacityToken = (typeof opacityTokens)[number];
export type DurationToken = (typeof durationTokens)[number];
export type EasingToken = (typeof easingTokens)[number];
export type TypeStyle = (typeof typeStyles)[number];
export type Affiliation = "friend" | "hostile" | "neutral" | "unknown";
/** CSS custom property reference for a token, e.g. cssVar("azure") -> "var(--azure)". */
export type CssVarToken =
  | ColorToken
  | SpacingToken
  | RadiusToken
  | ShadowToken
  | BlurToken
  | OpacityToken
  | DurationToken
  | EasingToken;
export const cssVar = (name: CssVarToken): string => \`var(--\${name})\`;
`;

const targets = [["tokens.css", css], ["tokens.ts", ts]];
let drift = false;
for (const [file, content] of targets) {
  const p = join(root, file);
  if (check) {
    let cur = ""; try { cur = readFileSync(p, "utf8"); } catch {}
    if (cur !== content) { console.error(`DRIFT: ${file} is out of date. Run: bun run gen:tokens`); drift = true; }
  } else writeFileSync(p, content);
}
if (check) process.exit(drift ? 1 : 0);
console.log("tokens generated");
