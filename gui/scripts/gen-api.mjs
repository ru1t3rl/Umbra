// Generates the GUI's API layer from the committed contract (F-5, skill generate-api-client):
//   ../api/openapi.json      -> features/data/src/generated/api.ts         (orval: react-query + fetch)
//   ../api/stream.schema.json -> features/engine-stream/src/generated/stream.d.ts (json-schema-to-typescript)
// Run: bun run gen:api   |   Check for drift: bun run gen:api --check
//   --api DIR        contract directory (default ../api)
//   --committed DIR  gui-shaped root whose generated files --check compares with (default this gui/)
// `--check` generates into a scratch location and fails on any difference; it never writes committed files.
import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { compileFromFile } from "json-schema-to-typescript";

const gui = join(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const flag = (name) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : undefined;
};
const check = args.includes("--check");
const apiDir = resolve(flag("--api") ?? join(gui, "..", "api"));
const committed = resolve(flag("--committed") ?? gui);

const HEADER = "// GENERATED, DO NOT EDIT. Run `bun run gen:api` (gui/scripts/gen-api.mjs).\n";
const dataDir = join(gui, "features", "data");
const scratch = mkdtempSync(join(tmpdir(), "umbra-gen-api-"));
// `--check` never writes under gui/: orval runs on a copy of the config and mutator files in the scratch
// dir (same relative layout, so the mutator import inside the output is byte-identical).
const workDir = check ? join(scratch, "features", "data") : dataDir;
const dataOut = join(workDir, "src", "generated");
const streamOut = check
  ? join(scratch, "stream-out", "stream.d.ts")
  : join(gui, "features", "engine-stream", "src", "generated", "stream.d.ts");

function cleanup() {
  rmSync(scratch, { recursive: true, force: true });
}

function genRest() {
  if (check) {
    mkdirSync(join(workDir, "src"), { recursive: true });
    // orval type-checks the mutator with the package's tsconfig, so the generated signatures depend on it.
    for (const f of [
      "package.json",
      "tsconfig.json",
      "orval.config.ts",
      "src/fetcher.ts",
      "src/problem.ts",
    ]) {
      copyFileSync(join(dataDir, f), join(workDir, f));
    }
    copyFileSync(join(gui, "tsconfig.base.json"), join(scratch, "tsconfig.base.json"));
    symlinkSync(join(gui, "node_modules"), join(scratch, "node_modules"), "dir");
    // orval reads package.json (react-query version) and the config imports "orval": resolve both here.
    symlinkSync(join(dataDir, "node_modules"), join(workDir, "node_modules"), "dir");
  }
  const r = spawnSync(
    join(dataDir, "node_modules", ".bin", "orval"),
    ["--config", "orval.config.ts"],
    {
      cwd: workDir,
      encoding: "utf8",
      env: {
        ...process.env,
        ORVAL_OUT: relative(workDir, dataOut),
        ORVAL_INPUT: relative(workDir, join(apiDir, "openapi.json")),
      },
    },
  );
  if (r.status !== 0) throw new Error(`orval failed:\n${r.stdout}${r.stderr}`);
  const file = join(dataOut, "api.ts");
  writeFileSync(file, HEADER + readFileSync(file, "utf8"));
}

async function genStream() {
  const ts = await compileFromFile(join(apiDir, "stream.schema.json"), {
    bannerComment: "",
    additionalProperties: false,
    cwd: apiDir,
  });
  mkdirSync(dirname(streamOut), { recursive: true });
  writeFileSync(streamOut, HEADER + ts);
}

function filesOf(dir) {
  if (!existsSync(dir)) return new Map();
  const out = new Map();
  const walk = (d) => {
    for (const e of readdirSync(d)) {
      const p = join(d, e);
      if (statSync(p).isDirectory()) walk(p);
      else out.set(relative(dir, p), readFileSync(p, "utf8"));
    }
  };
  walk(dir);
  return out;
}

function diff(label, expected, actual) {
  const problems = [];
  for (const [name, text] of expected) {
    if (!actual.has(name)) problems.push(`${label}/${name}: missing`);
    else if (actual.get(name) !== text) problems.push(`${label}/${name}: differs`);
  }
  for (const name of actual.keys()) {
    if (!expected.has(name)) problems.push(`${label}/${name}: not generated`);
  }
  return problems;
}

try {
  genRest();
  await genStream();
  if (check) {
    const problems = [
      ...diff(
        "features/data/src/generated",
        filesOf(dataOut),
        filesOf(join(committed, "features", "data", "src", "generated")),
      ),
      ...diff(
        "features/engine-stream/src/generated",
        filesOf(dirname(streamOut)),
        filesOf(join(committed, "features", "engine-stream", "src", "generated")),
      ),
    ];
    if (problems.length > 0) {
      throw new Error(
        `gen:api --check failed, run \`bun run gen:api\`:\n  ${problems.join("\n  ")}`,
      );
    }
    console.log("gen:api: generated files are up to date");
  } else {
    console.log("gen:api: wrote features/data/src/generated, features/engine-stream/src/generated");
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
} finally {
  cleanup();
}
