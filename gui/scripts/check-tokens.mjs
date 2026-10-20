// Fails on hard-coded design values outside ui-kit's tokens. Run: bun run check:tokens
// Escape hatch for a justified exception: add `umbra-allow` in a comment on the same line.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";
// `--root DIR` points the check at another tree with the same layout (tests use it for fixtures).
const argRoot = process.argv.indexOf("--root");
if (argRoot > 0 && !process.argv[argRoot + 1]) { console.error("usage: check-tokens.mjs [--root DIR]"); process.exit(2); }
const gui = argRoot > 0 ? process.argv[argRoot + 1] : join(dirname(fileURLToPath(import.meta.url)), "..");
const roots = ["features", "feature-shells", "src"].map((r) => join(gui, r));
const skip = (p) => /node_modules|[\\/]dist[\\/]|[\\/]generated[\\/]|[\\/]reference[\\/]|ui-kit[\\/]src[\\/]tokens[\\/]|\.test\.|\.spec\.|[\\/]e2e[\\/]/.test(p);
const rules = [
  ["hex colour literal", /#(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{3,4})\b(?![\w-])/],
  ["rgb/hsl/oklch colour literal", /\b(?:rgba?|hsla?|oklch|oklab)\(/],
  ["literal font-family (use var(--font-*))", /font-family\s*:\s*(?!var\(|inherit)/],
  ["literal font-size (use a .type-* class)", /font-size\s*:\s*\d/],
  ["literal border-radius (use var(--radius-*))", /border-radius\s*:\s*(?!0\b|50%|inherit|var\()/],
  ["literal box-shadow (use var(--shadow-*))", /box-shadow\s*:\s*(?!none|var\()/],
];
const files = [];
const walk = (d) => { for (const n of readdirSync(d)) { const p = join(d, n); if (skip(p + sep)) continue; const s = statSync(p); s.isDirectory() ? walk(p) : /\.(tsx?|css)$/.test(n) && files.push(p); } };
let missing = 0;
roots.forEach((r) => { try { walk(r); } catch { missing++; } }); // a missing root is counted, never silent
let bad = 0;
for (const f of files) readFileSync(f, "utf8").split("\n").forEach((line, i) => {
  if (line.includes("umbra-allow")) return;
  for (const [name, re] of rules) if (re.test(line)) { console.error(`${relative(gui, f)}:${i + 1}  ${name}\n    ${line.trim()}`); bad++; }
});
if (!files.length) { console.error(`no files scanned (${missing} of ${roots.length} roots missing): the check would pass vacuously`); process.exit(1); }
console.log(bad ? `\n${bad} design-token violation(s)` : `design tokens OK (${files.length} files${missing ? `, ${missing} roots missing` : ""})`);
process.exit(bad ? 1 : 0);
