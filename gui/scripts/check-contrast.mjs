// Verifies the WCAG contrast claims of the Vantage tokens in both themes. Run: bun run check:contrast
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
const t = JSON.parse(readFileSync(join(dirname(fileURLToPath(import.meta.url)), "..", "features", "ui-kit", "src", "tokens", "tokens.json"), "utf8"));

const raw = (name, theme) => { const c = t.color.tokens.find((x) => x.name === name); const v = typeof c.value === "object" ? c.value[theme] : c.value; const m = /^\{([\w-]+)\}$/.exec(v); return m ? raw(m[1], theme) : v; };
const parse = (s) => { s = s.trim(); if (s[0] === "#") { const h = s.slice(1); const f = h.length === 3 ? [...h].map((x) => x + x).join("") : h; return [0, 2, 4].map((i) => parseInt(f.slice(i, i + 2), 16)).concat(1); } const m = s.match(/[\d.]+/g).map(Number); return [m[0], m[1], m[2], m[3] ?? 1]; };
const over = (fg, bg) => { const a = fg[3]; return [0, 1, 2].map((i) => fg[i] * a + bg[i] * (1 - a)).concat(1); };
const lum = ([r, g, b]) => { const f = [r, g, b].map((v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; }); return 0.2126 * f[0] + 0.7152 * f[1] + 0.0722 * f[2]; };
const ratio = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };

let fail = 0, warn = 0;
const row = (theme, label, r, min, soft) => { const ok = r >= min; if (!ok) soft ? warn++ : fail++; console.log(`${ok ? "pass" : soft ? "WARN" : "FAIL"}  ${theme.padEnd(5)} ${label.padEnd(44)} ${r.toFixed(2)} (min ${min})`); };

for (const theme of ["dark", "light"]) {
  const C = (n) => parse(raw(n, theme));
  const surf = ["surface-0", "surface-1", "surface-2", "surface-3"];
  for (const s of surf) for (const ink of ["ink", "ink-muted", "azure", "cyan", "success", "danger", "info", "warning"]) row(theme, `${ink} on ${s}`, ratio(C(ink), C(s)), 4.5);
  for (const s of surf.slice(0, 3)) row(theme, `ink-faint on ${s}`, ratio(C("ink-faint"), C(s)), 4.5);
  for (const s of surf.slice(0, 3)) row(theme, `line-strong on ${s} (UI edge)`, ratio(C("line-strong"), C(s)), 3);
  for (const s of surf.slice(0, 3)) row(theme, `focus-ring on ${s}`, ratio(C("focus-ring"), C(s)), 3);
  for (const [fg, bg] of [["on-azure", "azure"], ["on-cyan", "cyan"], ["on-warning", "warning"]]) row(theme, `${fg} on ${bg} fill`, ratio(C(fg), C(bg)), 4.5);
  for (const d of ["azure", "cyan", "success", "danger", "warning"]) for (const s of ["surface-1", "surface-2"]) row(theme, `${d} on ${d}-dim over ${s}`, ratio(C(d), over(C(`${d}-dim`), C(s))), 4.5);
  for (const s of surf.slice(0, 3)) row(theme, `symbol-frame on ${s}`, ratio(C("symbol-frame"), C(s)), 3);
  for (const a of ["affil-friend", "affil-hostile", "affil-neutral", "affil-unknown"]) row(theme, `${a} fill on surface-1 (frame carries it)`, ratio(C(a), C("surface-1")), 3, true);
  const worst = theme === "dark" ? [107, 122, 140, 1] : [16, 24, 32, 1]; // worst-case map pixel behind a glass panel
  const glass = over(C("glass"), worst);
  row(theme, "ink on glass over worst-case map", ratio(C("ink"), glass), 4.5, true);
  row(theme, "ink-muted on glass over worst-case map", ratio(C("ink-muted"), glass), 4.5, true);
}
console.log(`\n${fail} failures, ${warn} warnings (WARN = informational: symbols rely on their frame; glass depends on the map behind it)`);
process.exit(fail ? 1 : 0);
