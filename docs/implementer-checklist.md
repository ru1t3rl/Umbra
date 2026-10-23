# Implementer checklist

Every engineer agent (Rust, plugin ABI, network, API, test) applies this before reporting a work item.
Each item is a review finding that came back in more than one M5 work item (WI-2..WI-4). Catching it here
saves a review-and-fix round. The code-reviewer uses the same list as its baseline.

## Tests
1. **Every key test can fail.** For the 4-6 behaviors that matter most, mutate the code yourself:
   delete the guard, flip the comparison, skip the call. Run the tests, restore the file exactly
   (check with `cmp` against a saved copy), and report each mutant as caught or survived, naming the
   failing test. Strengthen any test whose mutant survived.
2. **No vacuous asserts.** Assert success (exit code, `Ok`, `is_some()`) before comparing outputs: two
   failed runs compare equal (`None == None`). A test named "X tick by tick" checks every tick; a test
   named "A and B agree" compares A's value with B's, not each with a constant.
3. **The test reaches the code it names.** If an earlier phase already makes the guard unreachable,
   test the guard with a unit test inside the new module. Fix the integration test's doc comment so it
   does not claim to reach the guard.
4. **Doc comments say only what the test asserts.**
5. **Scratch files go under `env!("CARGO_TARGET_TMPDIR")`**, never `std::env::temp_dir()` or `/tmp`
   (a 16 GB tmpfs). No OS-specific paths such as `/dev/null`: write an empty file instead.
6. **Edge rows**: invalid input in every mode the code has (a single file and a directory; an explicit
   flag and discovery), not only the happy mode.

## Code
7. **Reuse before writing.** Grep for an existing reader, parser, validator or helper first. If a frozen
   file has the helper, widening its visibility to `pub(crate)` above the first `#[cfg(test)]` is
   allowed (additive). Never copy it.
8. **One helper for near-duplicate loops** (pass the per-item check as a function).
9. **Messages name what the user wrote.** When a value came from a default, say so.
10. **Compile-time over runtime** for invariants of constants (`const` with `match` and `panic!`), no
    dead `unwrap_or` fallbacks.
11. **Docs describe the present.** Do not document a feature of a later WI as if it existed; write
    "(from WI-n)". Update stale doc comments the change makes false (comments only, in frozen files).

## Frozen code (A-001, `scripts/m2-compat.sh`)
12. Additions only. In M2-era source files, every edit goes above the first `#[cfg(test)]`. Existing
    files under `crates/*/tests/`, `fixtures/` and `examples/` are never edited.
13. A new public field on an exhaustive struct: check that nothing outside the crate builds or
    destructures it, and record that in the work item's ASM.

## Verifying and reporting
14. **During the work item**, run `scripts/gates.sh --quick <crate>...` for the crates you touched, plus
    `sh scripts/m2-compat.sh` if you touched an M0-M4 file.
15. **Before reporting**, run `scripts/gates.sh` once on the final tree and paste its summary lines.
    Do not paste whole logs; they are in `target/gates/`.
16. **Report:**
    - `git status --short` and `git diff --stat`;
    - the verbatim diff of every frozen file you touched;
    - each planned test name mapped to the test you wrote;
    - the mutation results;
    - new ASMs;
    - anything not done.
    Claim only edits you made: the coordinator diff-checks every claim.
17. **Brief first.** When the prompt carries a work-item brief (plan section, AT rows, ADR extracts),
    work from it. Do not read the whole plan, `docs/SPEC.md` or `docs/assumptions/`; grep them for a
    specific ID when the brief does not cover it.
18. **No jest-dom matchers on large DOMs** (e.g. `toBeDisabled`, `toHaveTextContent`). A failing one prints the
    whole DOM into its message and can exhaust memory (bun reached 25 GB). Assert the property directly:
    `expect((el as HTMLButtonElement).disabled).toBe(true)`. Run `bun test` and Playwright under
    `systemd-run --user --scope -p MemoryMax=6G -- <cmd>`; exit 137 means the cap was hit: stop and report.
