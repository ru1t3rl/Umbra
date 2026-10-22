#!/bin/sh
# M0-M2 freeze proof, checklist items 1-3 and 2b (docs/plans/M3.md, A-001 constraints 1-5; AT-18),
# rebased on the rename (ADR 0018, Migration 4). Needs git, cargo, awk and the tags m2-done, umbra-rename.
#
#   scripts/m2-compat.sh [--keep] [BASELINE]   run the checks; exit 0 = all pass
#   scripts/m2-compat.sh --self-test           prove checks 1, 2 and 2b can fail (scratch worktree)
#
# Two tags (ADR 0018): CONTENT baseline = BASELINE, default umbra-rename (the rename commit; checks 1, 2
# and 2b compare the working tree to its content with a plain `git diff`). SCOPE = the files that existed
# at m2-done, with the path rule P1 applied (crates/cgf-<x>/ becomes crates/umbra-<x>/), so the frozen
# set stays M0-M2 (C10) and M3 files are not covered. Check 3 reads the test names at m2-done and maps
# the old package and binary names (B1/B2) before comparing.
#
# 1.  M2 fixtures and the walking-skeleton example: no modified, deleted or renamed file (new
#     files are allowed, M3 adds fixtures).
# 2.  No M1/M2 test or benchmark file modified, deleted or renamed.
# 2b. In an M2-era `src` file, no line from the first `#[cfg(test)]` onward (the whole file for
#     `tests.rs`) is removed, changed or added to (an inserted `#[ignore]` or early `return;`
#     would disable a test). The ADR 0011 edit of `all_lists_every_kind_once` in
#     crates/umbra-core/src/registry.rs is part of the baseline. The only allowlist is the H4 edit
#     of the same test (M6 WI-3, human decision H4, ASM-266): in that file, the removed line
#     `const COUNT: usize = 6;` and the added lines `const COUNT: usize = 7;` and
#     `ComponentKind::NetworkAdapter => 6,`, each matched exactly and at most once. Any other
#     changed or added line, in that test or elsewhere, still fails.
# 3.  The M2 test-name baseline, regenerated from m2-done in a scratch worktree under target/ (the
#     default target directory, never /tmp), is a subset of the test names now, keyed by
#     package, test binary and name, after mapping cgf-<x> to umbra-<x>, cgf_<x> to umbra_<x> and the
#     cgf-cli unit-test binary cgf_cli to umbra; the baseline build must succeed and list >= 900 tests. A
#     test that was not ignored at the tag must not be ignored now (`--list --ignored` compared).
set -eu

cd "$(dirname "$0")/.."
root=$(pwd)
work="$root/target/m2-compat"
keep=0
self_test=0
tag=umbra-rename
scope_tag=m2-done
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
trap cleanup EXIT
trap 'exit 130' INT TERM

git rev-parse --verify -q "$tag^{commit}" >/dev/null || { echo "FAIL: unknown tag $tag"; exit 1; }
git rev-parse --verify -q "$scope_tag^{commit}" >/dev/null || { echo "FAIL: unknown tag $scope_tag"; exit 1; }

# p1: the path rule P1 of ADR 0018.
p1() { sed 's#^crates/cgf-#crates/umbra-#'; }
# rename_names: package and binary names of m2-done to the current ones (B1, B2; cgf_cli unit tests -> umbra).
rename_names() { sed -e 's/^cgf-cli::cgf_cli::/umbra-cli::umbra::/' -e 's/cgf-/umbra-/g' -e 's/cgf_/umbra_/g'; }

# scope_files SCOPE_TAG EREGEX: the files of SCOPE_TAG matching the regex, with P1 applied.
scope_files() { git ls-tree -r --name-only "$1" | grep -E "$2" | p1; }

# changed_files CONTENT_TAG FILES: files (one per line) that are modified, deleted or renamed against CONTENT_TAG.
changed_files() {
    _t=$1; _f=$2
    [ -n "$_f" ] || return 0
    printf '%s\n' "$_f" | xargs git diff --name-only --diff-filter=MDR "$_t" --
}

# check_1_2 CONTENT_TAG SCOPE_TAG: prints failures, returns 1 on any. Runs in the current directory (a git tree).
check_1_2() {
    _tag=$1; _scope=$2; _bad=0
    echo "== 1. M2 fixtures and the walking-skeleton example unchanged since $_tag (scope $_scope)"
    _c=$(changed_files "$_tag" "$(scope_files "$_scope" '^(crates/[^/]+/tests/fixtures/|examples/scenarios/walking_skeleton(/|$))')")
    if [ -n "$_c" ]; then echo "FAIL: changed fixture files:"; echo "$_c"; _bad=1; else echo ok; fi
    echo "== 2. M1/M2 test and bench files unchanged since $_tag (scope $_scope)"
    _c=$(changed_files "$_tag" "$(scope_files "$_scope" '^crates/[^/]+/(tests|benches)/')")
    if [ -n "$_c" ]; then echo "FAIL: changed test files:"; echo "$_c"; _bad=1; else echo ok; fi
    return "$_bad"
}

# check_2b CONTENT_TAG SCOPE_TAG: inline test regions of M2-era src files. Runs in the current directory.
check_2b() {
    _tag=$1; _scope=$2; _bad=0
    echo "== 2b. inline #[cfg(test)] regions of M2-era src files unchanged since $_tag (scope $_scope)"
    for f in $(git ls-tree -r --name-only "$_scope" -- crates | grep '/src/.*\.rs$' | p1); do
        [ -f "$f" ] || { echo "FAIL: $f was deleted"; _bad=1; continue; }
        case "$f" in
            */tests.rs) start=1 ;;
            *) start=$(git show "$_tag:$f" | grep -n '#\[cfg(test)\]' | head -n 1 | cut -d: -f1 || true) ;;
        esac
        [ -n "${start:-}" ] || continue
        _out=$(git diff -U0 "$_tag" -- "$f" | awk -v start="$start" -v file="$f" '
            BEGIN {  # H4 allowlist (ASM-266): exact diff lines, each usable once, registry.rs only
                if (file == "crates/umbra-core/src/registry.rs") {
                    allow["-        const COUNT: usize = 6;"] = 1
                    allow["+        const COUNT: usize = 7;"] = 1
                    allow["+            ComponentKind::NetworkAdapter => 6,"] = 1
                }
            }
            /^@@/ { split($2, a, ","); old = substr(a[1], 2) + 0; if (a[2] == "0") old++; next }
            /^---/ || /^\+\+\+/ { next }
            /^-/ { if (old >= start && allow[$0] > 0) { allow[$0]--; old++; next }
                   if (old >= start) print file ":" old ": " $0; old++; next }
            /^\+/ { if (old > start && allow[$0] > 0) { allow[$0]--; next }
                   if (old > start) print file ":" old ": " $0 }')
        if [ -n "$_out" ]; then echo "FAIL: changed or added test-region lines:"; echo "$_out"; _bad=1; fi
    done
    [ "$_bad" -eq 0 ] && echo ok
    return "$_bad"
}

# list_pkg DIR TARGET_DIR PKG RAW [ARGS...]: raw `--list` output of one package; fails if cargo does.
list_pkg() {
    (
        cd "$1"
        _target=$2; _pkg=$3; _raw=$4; shift 4
        CARGO_TARGET_DIR="$_target" cargo test -p "$_pkg" --locked -- --list "$@" > "$_raw" 2>&1 \
            || { echo "FAIL: cargo test -p $_pkg --list failed in $1"; tail -n 20 "$_raw"; return 1; }
    )
}

# list DIR TARGET_DIR OUTFILE [ARGS...]: "<package>::<binary>::<test>" for every listed test.
list() {
    _dir=$1; _target=$2; _out=$3; shift 3
    : > "$_out"
    for pkg in $(cd "$_dir" && ls crates) xtask; do
        list_pkg "$_dir" "$_target" "$pkg" "$_out.raw" "$@" || return 1
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
            }' "$_out.raw" >> "$_out"
    done
    sort -u "$_out" -o "$_out"
    rm -f "$_out.raw"
}

check_floor() { # check_floor COUNT
    if [ "$1" -lt 900 ]; then echo "FAIL: baseline lists fewer than 900 tests"; return 1; fi
}

# check_ignored M2_TESTS M2_IGNORED NOW_IGNORED: no test of the tag is ignored now unless it was then.
check_ignored() {
    _new=$(comm -12 "$3" "$1" | comm -23 - "$2")
    if [ -n "$_new" ]; then echo "FAIL: tests ignored now that were not ignored at the tag:"; echo "$_new"; return 1; fi
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
    expect PASS "clean tree, checks 1 and 2" check_1_2 "$tag" "$scope_tag"
    expect PASS "clean tree, check 2b" check_2b "$tag" "$scope_tag"
    echo "// mutation" >> crates/umbra-core/tests/ids.rs
    expect FAIL "modified M2 test file" check_1_2 "$tag" "$scope_tag"
    git checkout -q -- crates/umbra-core/tests/ids.rs
    rm -f crates/umbra-engine/tests/determinism.rs
    expect FAIL "deleted M2 test file" check_1_2 "$tag" "$scope_tag"
    git checkout -q -- crates/umbra-engine/tests/determinism.rs
    echo "# mutation" >> examples/scenarios/walking_skeleton/README.md
    expect FAIL "modified walking-skeleton file" check_1_2 "$tag" "$scope_tag"
    git checkout -q -- examples/scenarios/walking_skeleton/README.md
    f=$(git ls-files 'crates/umbra-templates/tests/fixtures/dirs/*' | head -n 1)
    echo "# mutation" >> "$f"
    expect FAIL "modified M2 fixture file" check_1_2 "$tag" "$scope_tag"
    git checkout -q -- "$f"
    # 2b: change the closing line of the inline test module of an M2-era src file.
    sed -i '$ s/}$/} \/\/ mutation/' crates/umbra-templates/src/diagnostic.rs
    expect FAIL "changed line in an inline test region" check_2b "$tag" "$scope_tag"
    git checkout -q -- .
    git checkout -q -- .
    # 2b rejects additions too: an inserted #[ignore] or an early return disables an M2 inline test.
    sed -i '0,/^    #\[test\]$/s//    #[test]\n    #[ignore]/' crates/umbra-templates/src/diagnostic.rs
    expect FAIL "#[ignore] inserted mid-module in an M2 inline test" check_2b "$tag" "$scope_tag"
    git checkout -q -- .
    sed -i '0,/fn display_is_path_line_col_error_code_message_then_notes() {/s//&\n        return;/' crates/umbra-templates/src/diagnostic.rs
    expect FAIL "return; inserted into an M2 inline test body" check_2b "$tag" "$scope_tag"
    git checkout -q -- .
    echo "// addition at the end of the test module" >> crates/umbra-templates/src/diagnostic.rs
    expect FAIL "line appended to an M2 inline test region" check_2b "$tag" "$scope_tag"
    git checkout -q -- .
    sed -i '0,/^#\[test\]$/s//#[test]\n#[ignore]/' crates/umbra-core/tests/ids.rs
    expect FAIL "M2 integration test newly marked #[ignore]" check_1_2 "$tag" "$scope_tag"
    git checkout -q -- .
    # Scope is M0-M2: a changed M3-era test file is not covered, and the ADR 0011 edit has no allowlist.
    echo "// mutation" >> crates/umbra-behavior/tests/composites.rs
    expect PASS "modified M3-era test file is out of scope" check_1_2 "$tag" "$scope_tag"
    git checkout -q -- .
    sed -i '0,/fn all_lists_every_kind_once/s//&\n        let _ = 0;/' crates/umbra-core/src/registry.rs
    expect FAIL "edit of the ADR 0011 test is no longer allowlisted" check_2b "$tag" "$scope_tag"
    git checkout -q -- .
    # H4 allowlist (ASM-266): applied here because the tag and, until WI-3 is committed, HEAD lack it.
    h4_apply() {
        sed -i -e 's/^        const COUNT: usize = 6;$/        const COUNT: usize = 7;/' \
            crates/umbra-core/src/registry.rs
        grep -q 'ComponentKind::NetworkAdapter => 6,' crates/umbra-core/src/registry.rs || \
            sed -i 's/^            ComponentKind::Action => 5,$/&\n            ComponentKind::NetworkAdapter => 6,/' crates/umbra-core/src/registry.rs
    }
    h4_apply
    expect PASS "H4 edit alone is allowlisted" check_2b "$tag" "$scope_tag"
    sed -i '0,/fn all_lists_every_kind_once/s//&\n        let _ = 0;/' crates/umbra-core/src/registry.rs
    expect FAIL "H4 edit plus one extra line in the same test" check_2b "$tag" "$scope_tag"
    git checkout -q -- .
    h4_apply
    sed -i 's/^            ComponentKind::NetworkAdapter => 6,$/&\n&/' crates/umbra-core/src/registry.rs
    expect FAIL "H4 arm added twice" check_2b "$tag" "$scope_tag"
    git checkout -q -- .
    echo "            ComponentKind::NetworkAdapter => 6," >> crates/umbra-templates/src/diagnostic.rs
    expect FAIL "H4 arm in another file" check_2b "$tag" "$scope_tag"
    git checkout -q -- .
    # Check 3 failure paths and the name mapping.
    expect PASS "name mapping: package, binary and unit-test binary" test "$(printf 'cgf-cli::cgf_cli::t\ncgf-core::cgf_core::u\ncgf-core::ids::cgf_x\n' | rename_names | tr '\n' ' ')" = "umbra-cli::umbra::t umbra-core::umbra_core::u umbra-core::ids::umbra_x "
    expect FAIL "list of a missing package" list_pkg "$work/selftest" "$work/selftest-target" no-such-package "$work/selftest.raw"
    expect FAIL "baseline below the floor" check_floor 10
    expect PASS "baseline at the floor" check_floor 900
    printf 'p::b::kept\np::b::gone\n' | sort > "$work/st-m2.txt"
    : > "$work/st-m2-ignored.txt"
    printf 'p::b::gone\n' > "$work/st-now-ignored.txt"
    expect FAIL "M2 test ignored now but not at the tag" check_ignored "$work/st-m2.txt" "$work/st-m2-ignored.txt" "$work/st-now-ignored.txt"
    cp "$work/st-now-ignored.txt" "$work/st-m2-ignored.txt"
    expect PASS "test already ignored at the tag" check_ignored "$work/st-m2.txt" "$work/st-m2-ignored.txt" "$work/st-now-ignored.txt"
    printf 'p::b::new_m3_test\n' > "$work/st-now-ignored.txt"
    : > "$work/st-m2-ignored.txt"
    expect PASS "new M3 test ignored" check_ignored "$work/st-m2.txt" "$work/st-m2-ignored.txt" "$work/st-now-ignored.txt"
    exit "$st"
fi

status=0
check_1_2 "$tag" "$scope_tag" || status=1
check_2b "$tag" "$scope_tag" || status=1

echo "== 3. every test name of $scope_tag still exists (names mapped) and is not newly ignored"
mkdir -p "$work"
git worktree prune
rm -rf "$work/tree"
git worktree add --detach "$work/tree" "$scope_tag"

list "$work/tree" "$work/baseline-target" "$work/m2-tests.txt" || status=1
list "$work/tree" "$work/baseline-target" "$work/m2-ignored.txt" --ignored || status=1
for k in m2-tests m2-ignored; do
    rename_names < "$work/$k.txt" | sort -u > "$work/$k.mapped" && mv "$work/$k.mapped" "$work/$k.txt"
done
list "$root" "$root/target" "$work/now-tests.txt" || status=1
list "$root" "$root/target" "$work/now-ignored.txt" --ignored || status=1

base=$(wc -l < "$work/m2-tests.txt")
echo "baseline: $base tests, now: $(wc -l < "$work/now-tests.txt") tests"
check_floor "$base" || status=1
missing=$(comm -23 "$work/m2-tests.txt" "$work/now-tests.txt")
if [ -n "$missing" ]; then
    echo "FAIL: tests of $scope_tag missing now:"; echo "$missing"; status=1
else
    echo "ok (only additions)"
fi
check_ignored "$work/m2-tests.txt" "$work/m2-ignored.txt" "$work/now-ignored.txt" || status=1

exit "$status"
