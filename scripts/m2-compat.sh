#!/bin/sh
# M3 compatibility proof, checklist items 1-3 and 2b (docs/plans/M3.md, A-001 constraints 1-5; AT-18).
# build-engineer runs it in WI-16 and WI-18. Needs git, cargo, awk and the tag m2-done.
#
#   scripts/m2-compat.sh [--keep] [TAG]    run the checks (default tag m2-done); exit 0 = all pass
#   scripts/m2-compat.sh --self-test       prove checks 1, 2 and 2b can fail (scratch worktree)
#
# 1.  M2 fixtures and the walking-skeleton example: no modified, deleted or renamed file (new
#     files are allowed, M3 adds fixtures).
# 2.  No M1/M2 test or benchmark file modified, deleted or renamed.
# 2b. In an M2-era `src` file, no line from the first `#[cfg(test)]` onward (the whole file for
#     `tests.rs`) is removed or changed; the one allowlisted change is the approved edit of
#     `all_lists_every_kind_once` in crates/cgf-core/src/registry.rs (ADR 0011).
# 3.  The M2 test-name baseline, regenerated from the tag in a scratch worktree under target/ (the
#     default target directory, never /tmp), is a subset of the test names now, keyed by
#     package, test binary and name; the baseline build must succeed and list >= 900 tests.
set -eu

cd "$(dirname "$0")/.."
root=$(pwd)
work="$root/target/m2-compat"
keep=0
self_test=0
tag=m2-done
for arg in "$@"; do
    case "$arg" in
        --keep) keep=1 ;;
        --self-test) self_test=1 ;;
        *) tag="$arg" ;;
    esac
done

cleanup() {
    cd "$root"
    for t in tree selftest; do
        if [ -d "$work/$t" ]; then git worktree remove --force "$work/$t" >/dev/null 2>&1 || rm -rf "$work/$t"; fi
    done
    git worktree prune
    if [ "$keep" -eq 0 ]; then rm -rf "$work/baseline-target"; fi
}
trap cleanup EXIT INT TERM

# check_1_2 TAG: prints failures, returns 1 on any. Runs in the current directory (a git tree).
check_1_2() {
    _tag=$1; _bad=0
    echo "== 1. fixtures and example unchanged since $_tag"
    _c=$(git diff --name-only --diff-filter=MDR "$_tag" -- \
        ':(glob)crates/*/tests/fixtures/**' 'examples/scenarios/walking_skeleton')
    if [ -n "$_c" ]; then echo "FAIL: changed fixture files:"; echo "$_c"; _bad=1; else echo ok; fi
    echo "== 2. M1/M2 test and bench files unchanged since $_tag"
    _c=$(git diff --name-only --diff-filter=MDR "$_tag" -- \
        ':(glob)crates/*/tests/**' ':(glob)crates/*/benches/**')
    if [ -n "$_c" ]; then echo "FAIL: changed test files:"; echo "$_c"; _bad=1; else echo ok; fi
    return "$_bad"
}

# check_2b TAG: inline test regions of M2-era src files. Runs in the current directory.
check_2b() {
    _tag=$1; _bad=0
    echo "== 2b. inline #[cfg(test)] regions of M2-era src files unchanged since $_tag"
    for f in $(git ls-tree -r --name-only "$_tag" -- crates | grep '/src/.*\.rs$'); do
        [ -f "$f" ] || { echo "FAIL: $f was deleted"; _bad=1; continue; }
        case "$f" in
            */tests.rs) start=1 ;;
            *) start=$(git show "$_tag:$f" | grep -n '#\[cfg(test)\]' | head -n 1 | cut -d: -f1 || true) ;;
        esac
        [ -n "${start:-}" ] || continue
        # Allowlisted range: the approved test edit (ADR 0011) at the tag's line numbers.
        if [ "$f" = crates/cgf-core/src/registry.rs ]; then
            range=$(git show "$_tag:$f" | awk '
                /fn all_lists_every_kind_once/ { s = NR }
                s && !e && /^    }$/ && NR > s { e = NR }
                END { if (s && e) print s, e }')
        else
            range="0 0"
        fi
        set -- $range
        _out=$(git diff -U0 "$_tag" -- "$f" | awk -v start="$start" -v allow_s="$1" -v allow_e="$2" -v file="$f" '
            /^@@/ { split($2, a, ","); old = substr(a[1], 2) + 0; if (a[2] == "0") old++; next }
            /^---/ || /^\+\+\+/ { next }
            /^-/ { if (old >= start && !(old >= allow_s && old <= allow_e)) print file ":" old ": " $0; old++ }')
        if [ -n "$_out" ]; then echo "FAIL: changed test-region lines:"; echo "$_out"; _bad=1; fi
    done
    [ "$_bad" -eq 0 ] && echo ok
    return "$_bad"
}

if [ "$self_test" -eq 1 ]; then
    mkdir -p "$work"
    git worktree prune
    rm -rf "$work/selftest"
    git worktree add --detach "$work/selftest" HEAD
    cd "$work/selftest"
    st=0
    expect() { # expect FAIL|PASS "description" command...
        want=$1; desc=$2; shift 2
        if "$@" >/dev/null 2>&1; then got=PASS; else got=FAIL; fi
        if [ "$got" = "$want" ]; then echo "self-test ok: $desc -> $got"; else echo "self-test WRONG: $desc -> $got (wanted $want)"; st=1; fi
    }
    expect PASS "clean tree, checks 1 and 2" check_1_2 "$tag"
    expect PASS "clean tree, check 2b" check_2b "$tag"
    echo "// mutation" >> crates/cgf-core/tests/ids.rs
    expect FAIL "modified M2 test file" check_1_2 "$tag"
    git checkout -q -- crates/cgf-core/tests/ids.rs
    rm -f crates/cgf-engine/tests/determinism.rs
    expect FAIL "deleted M2 test file" check_1_2 "$tag"
    git checkout -q -- crates/cgf-engine/tests/determinism.rs
    echo "# mutation" >> examples/scenarios/walking_skeleton/README.md
    expect FAIL "modified walking-skeleton file" check_1_2 "$tag"
    git checkout -q -- examples/scenarios/walking_skeleton/README.md
    f=$(git ls-files 'crates/cgf-templates/tests/fixtures/*' | head -n 1)
    echo "# mutation" >> "$f"
    expect FAIL "modified M2 fixture file" check_1_2 "$tag"
    git checkout -q -- "$f"
    # 2b: change the closing line of the inline test module of an M2-era src file.
    sed -i '$ s/}$/} \/\/ mutation/' crates/cgf-templates/src/diagnostic.rs
    expect FAIL "changed line in an inline test region" check_2b "$tag"
    git checkout -q -- .
    echo "// addition only" >> crates/cgf-templates/src/diagnostic.rs
    expect PASS "addition at the end of a test region is allowed" check_2b "$tag"
    exit "$st"
fi

status=0
check_1_2 "$tag" || status=1
check_2b "$tag" || status=1

echo "== 3. every test name of $tag still exists"
mkdir -p "$work"
git worktree prune
rm -rf "$work/tree"
git worktree add --detach "$work/tree" "$tag"

# list DIR TARGET_DIR OUTFILE: "<package>::<binary>::<test>" for every listed test.
list() {
    (
        cd "$1"
        : > "$3"
        for pkg in $(ls crates) xtask; do
            CARGO_TARGET_DIR="$2" cargo test -p "$pkg" --locked -- --list > "$3.raw" 2>&1 \
                || { echo "FAIL: cargo test -p $pkg --list failed in $1"; tail -n 20 "$3.raw"; return 1; }
            awk -v pkg="$pkg" '
                /^ *(Running|Doc-tests) / {
                    bin = $0
                    sub(/^ *Running .*deps\//, "", bin)
                    sub(/-[0-9a-f]+\)$/, "", bin)
                    if ($1 == "Doc-tests") { bin = "doctest" }
                    next
                }
                /: test$/ {
                    name = $0
                    sub(/: test$/, "", name)
                    sub(/ \(line [0-9]+\)$/, "", name)   # doctests are named by source line
                    print pkg "::" bin "::" name
                }' "$3.raw" >> "$3"
        done
        sort -u "$3" -o "$3"
        rm -f "$3.raw"
    )
}

list "$work/tree" "$work/baseline-target" "$work/m2-tests.txt" || status=1
list "$root" "$root/target" "$work/now-tests.txt" || status=1

base=$(wc -l < "$work/m2-tests.txt")
echo "baseline: $base tests, now: $(wc -l < "$work/now-tests.txt") tests"
if [ "$base" -lt 900 ]; then echo "FAIL: baseline lists fewer than 900 tests"; status=1; fi
missing=$(comm -23 "$work/m2-tests.txt" "$work/now-tests.txt")
if [ -n "$missing" ]; then
    echo "FAIL: tests of $tag missing now:"; echo "$missing"; status=1
else
    echo "ok (only additions)"
fi

exit "$status"
