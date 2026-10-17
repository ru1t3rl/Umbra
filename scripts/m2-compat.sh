#!/bin/sh
# M3 compatibility proof, checklist items 1-3 (docs/plans/M3.md, A-001 constraints 1-5; AT-18).
# Run from anywhere: build-engineer runs it in WI-16 and WI-18. Needs git, cargo and the tag
# m2-done (or pass another tag as $1). Exit 0 when every check passes, 1 otherwise.
#
# 1. M2 fixtures and the walking-skeleton example are unchanged (no modified, deleted or renamed
#    file; new files are allowed because M3 adds fixtures).
# 2. No M1/M2 test or benchmark file is modified, deleted or renamed.
# 3. The M2 test-name baseline, regenerated from the tag in a scratch worktree under target/ (the
#    default target directory, never /tmp), is a subset of the test names now (per test binary).
set -eu

cd "$(dirname "$0")/.."
tag="${1:-m2-done}"
status=0

echo "== 1. fixtures and example unchanged since $tag"
changed=$(git diff --name-only --diff-filter=MDR "$tag" -- examples/scenarios/walking_skeleton 'crates/*/tests/fixtures')
if [ -n "$changed" ]; then
    echo "FAIL: changed fixture files:"; echo "$changed"; status=1
else
    echo "ok"
fi

echo "== 2. M1/M2 test and bench files unchanged since $tag"
changed=$(git diff --name-only --diff-filter=MDR "$tag" -- 'crates/*/tests' 'crates/*/benches')
if [ -n "$changed" ]; then
    echo "FAIL: changed test files:"; echo "$changed"; status=1
else
    echo "ok"
fi

echo "== 3. every test name of $tag still exists"
work="target/m2-compat"
rm -rf "$work/tree"
mkdir -p "$work"
git worktree prune
git worktree add --detach "$work/tree" "$tag" >/dev/null 2>&1

list() {
    # Prints "<test binary>::<test name>" for every test that cargo lists in the workspace.
    CARGO_TARGET_DIR="$1" cargo test --workspace --locked -- --list 2>&1 | awk '
        /^ *(Running|Doc-tests) / {
            bin = $0
            sub(/^ *Running .*deps\//, "", bin)
            sub(/-[0-9a-f]+\)$/, "", bin)
            if ($1 == "Doc-tests") { bin = "doctest-" $2 }
            next
        }
        /: test$/ {
            name = $0
            sub(/: test$/, "", name)
            # Doctests are named by source line, which shifts when items are added above them.
            sub(/ \(line [0-9]+\)$/, "", name)
            print bin "::" name
        }
    ' | sort -u
}

(cd "$work/tree" && list "$(pwd)/../baseline-target") > "$work/m2-tests.txt"
list "$(pwd)/target" > "$work/now-tests.txt"
git worktree remove --force "$work/tree"

echo "baseline: $(wc -l < "$work/m2-tests.txt") tests, now: $(wc -l < "$work/now-tests.txt") tests"
missing=$(comm -23 "$work/m2-tests.txt" "$work/now-tests.txt")
if [ -n "$missing" ]; then
    echo "FAIL: tests of $tag missing now:"; echo "$missing"; status=1
else
    echo "ok (only additions)"
fi

exit "$status"
