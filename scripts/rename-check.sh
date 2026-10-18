#!/bin/sh
# Mechanical-rename verification of ADR 0018 (D-35, A-002). Needs git, cargo, sed, awk, grep, diff, cmp.
# build-engineer runs it in WI-R2 (and the self-test in WI-R1). Exempt from the rename (E4).
#
#   scripts/rename-check.sh [--keep] [--allow PATH]... OLD NEW   verify NEW against OLD (tags or commits)
#   scripts/rename-check.sh --self-test                           prove the check can fail
#
# What it does (ADR 0018, "Compatibility and migration" item 5):
#   1. Builds the expected tree: a scratch worktree of OLD under target/rename-check/ with the path rules
#      P1-P2 (git mv), scripts/rename-map.sed on every tracked file except E1-E5, cargo fmt --all, the
#      lock steps (root: cargo metadata --offline; fixtures: cargo generate-lockfile --offline) and the
#      allowed hunk M1 (the [[bin]] entry appended to crates/umbra-cli/Cargo.toml).
#   2. diff -r expected tree vs NEW (without .git and target). The only tolerated differences are files
#      named with --allow (hunk M2: an xtask test list reordered); they are printed for the review.
#   3. E1-E5 are byte-identical in OLD and NEW.
#   4. Leftover scan of NEW: no cgf/Cgf outside E1-E5 and no path containing cgf; upper-case CGF only at
#      the two group D places (docs/SPEC.md, .claude/agents/architect.md; two lines in all).
#   5. cargo test --workspace -- --list (and --ignored) at OLD and NEW are equal after mapping names.
#   6. Cargo.lock: same package count, same external entries (name, version, source, checksum).
#   7. umbra vs cgf-cli on both reference scenarios: run --hash-every 1 stdout byte-identical, same exit
#      status; validate and schema outputs equal (old output passed through the rename map, so the
#      file-format schemas differ only by the mapped title); schema component for every built-in id
#      identical; cargo xtask check-deps parity on the real workspace and two fixtures.
# Exit 0 = all pass. Scratch space is target/rename-check (never /tmp).
set -eu

cd "$(dirname "$0")/.."
root=$(pwd)
work="$root/target/rename-check"
map="scripts/rename-map.sed"
excl="docs/adr/0018-rename-the-project-to-umbra.md docs/amendments/A-002-umbra-and-visual-timeline.md docs/amendments/spec-v0.1-to-v0.2.patch scripts/rename-check.sh scripts/rename-map.sed"
keep=0
self_test=0
allow=""
pos=""
while [ $# -gt 0 ]; do
    case "$1" in
        --keep) keep=1 ;;
        --self-test) self_test=1 ;;
        --allow) shift; allow="$allow $1" ;;
        *) pos="$pos $1" ;;
    esac
    shift
done

cleanup() {
    cd "$root"
    for t in expected old new st-old st-expected st-actual; do
        if [ -d "${work:?}/$t" ]; then git worktree remove --force "${work:?}/$t" >/dev/null 2>&1 || rm -rf "${work:?}/${t:?}"; fi
    done
    git worktree prune
    if [ "$keep" -eq 0 ]; then rm -rf "${work:?}/old-target" "${work:?}/new-target"; fi
}
trap cleanup EXIT
trap 'exit 130' INT TERM

is_excluded() { # is_excluded PATH
    for _e in $excl; do [ "$1" = "$_e" ] && return 0; done
    return 1
}

# rename_text: the rename map as a filter (used on output of the old binary).
rename_text() { sed -E -f "$root/$map"; }

# apply_rename DIR: the whole rename of ADR 0018 (P1, P2, rules, steps 1-3, hunk M1) in a git worktree.
apply_rename() {
    (
        cd "$1"
        for d in crates/cgf-*; do
            git mv "$d" "crates/umbra-${d#crates/cgf-}"
        done
        for d in xtask/tests/fixtures/*/crates/cgf-*; do
            [ -d "$d" ] || continue
            git mv "$d" "$(dirname "$d")/umbra-${d##*/cgf-}"
        done
        _left=$(git ls-files | grep -i cgf || true)
        if [ -n "$_left" ]; then echo "FAIL: tracked paths still containing cgf:"; echo "$_left"; exit 1; fi
        git ls-files | while IFS= read -r f; do
            is_excluded "$f" && continue
            [ -f "$f" ] && [ ! -L "$f" ] || continue
            sed -i -E -f "$root/$map" "$f"
        done
        cargo fmt --all
        cargo metadata --offline --format-version 1 > /dev/null
        for lock in xtask/tests/fixtures/*/Cargo.lock; do
            [ -f "$lock" ] || continue
            (cd "$(dirname "$lock")" && cargo generate-lockfile --offline)
        done
        printf '\n[[bin]]\nname = "umbra"\npath = "src/main.rs"\n' >> crates/umbra-cli/Cargo.toml
    )
}

# check_tree EXPECTED ACTUAL: no difference except files named by --allow (hunk M2).
check_tree() {
    _out=$(diff -rq -x .git -x target "$1" "$2" || true)
    _bad=""
    if [ -n "$_out" ]; then
        _bad=$(printf '%s\n' "$_out" | while IFS= read -r line; do
            ok=0
            for a in $allow; do
                case "$line" in *"/$a "*|*"/$a") ok=1 ;; esac
            done
            [ "$ok" -eq 1 ] || printf '%s\n' "$line"
        done)
    fi
    if [ -n "$_bad" ]; then
        echo "FAIL: ACTUAL differs from the expected tree:"; printf '%s\n' "$_bad"; return 1
    fi
    for a in $allow; do
        echo "allowed hunk (M2) in $a:"; diff -u "$1/$a" "$2/$a" || true
    done
    return 0
}

# check_exact OLDDIR ACTUALDIR: E1-E5 byte-identical.
check_exact() {
    _bad=0
    for e in $excl; do
        if ! cmp -s "$1/$e" "$2/$e"; then echo "FAIL: $e differs from the old tree"; _bad=1; fi
    done
    return "$_bad"
}

# check_leftover DIR: leftover scan.
check_leftover() {
    _bad=0
    _hits=$(grep -rIn -e cgf -e Cgf --exclude-dir=.git --exclude=.git --exclude-dir=target "$1" || true)
    for e in $excl; do _hits=$(printf '%s\n' "$_hits" | grep -v "^$1/$e:" || true); done
    if [ -n "$_hits" ]; then echo "FAIL: leftover cgf/Cgf:"; printf '%s\n' "$_hits" | head -n 40; _bad=1; fi
    _p=$(find "$1" -mindepth 1 \( -name .git -o -name target \) -prune -o -iname '*cgf*' -print)
    if [ -n "$_p" ]; then echo "FAIL: paths containing cgf:"; echo "$_p"; _bad=1; fi
    _up=$(grep -rIn CGF --exclude-dir=.git --exclude=.git --exclude-dir=target "$1" || true)
    for e in $excl; do _up=$(printf '%s\n' "$_up" | grep -v "^$1/$e:" || true); done
    _n=0; [ -z "$_up" ] || _n=$(printf '%s\n' "$_up" | wc -l)
    _files=$(printf '%s\n' "$_up" | cut -d: -f1 | sed "s#^$1/##" | sort -u | tr '\n' ' ')
    if [ "$_n" -ne 2 ] || [ "$_files" != ".claude/agents/architect.md docs/SPEC.md " ]; then
        echo "FAIL: upper-case CGF must occur exactly twice (docs/SPEC.md, .claude/agents/architect.md); found $_n:"
        printf '%s\n' "$_up"; _bad=1
    else
        echo "group D (kept):"; printf '%s\n' "$_up"
    fi
    return "$_bad"
}

if [ "$self_test" -eq 1 ]; then
    mkdir -p "$work"
    git worktree prune
    for t in st-old st-expected st-actual; do rm -rf "${work:?}/${t:?}"; done
    git worktree prune
    git worktree add --detach "$work/st-old" HEAD >/dev/null
    git worktree add --detach "$work/st-expected" HEAD >/dev/null
    apply_rename "$work/st-expected"
    cp -a "$work/st-expected" "$work/st-actual"
    old="$work/st-old"; exp="$work/st-expected"; act="$work/st-actual"
    st=0
    expect() { # expect FAIL|PASS "description" command...
        want=$1; desc=$2; shift 2
        if "$@" >/dev/null 2>&1; then got=PASS; else got=FAIL; fi
        if [ "$got" = "$want" ]; then echo "self-test ok: $desc -> $got"; else echo "self-test WRONG: $desc -> $got (wanted $want)"; st=1; fi
    }
    restore() { cp -a "$exp/$1" "$act/$1"; }
    expect PASS "clean rename, tree" check_tree "$exp" "$act"
    expect PASS "clean rename, E1-E5" check_exact "$old" "$act"
    expect PASS "clean rename, leftover scan" check_leftover "$act"

    f=crates/umbra-core/src/lib.rs
    echo "// planted extra edit" >> "$act/$f"
    expect FAIL "planted extra edit" check_tree "$exp" "$act"
    restore "$f"

    f=crates/umbra-engine/src/lib.rs
    sed -i '1i use cgf_core::planted;' "$act/$f"
    expect FAIL "missed cgf_ (tree diff)" check_tree "$exp" "$act"
    expect FAIL "missed cgf_ (leftover scan)" check_leftover "$act"
    restore "$f"

    f=README.md
    echo "The CGF Engine" >> "$act/$f"
    expect FAIL "extra upper-case CGF (leftover scan)" check_leftover "$act"
    restore "$f"

    f=docs/adr/0018-rename-the-project-to-umbra.md
    echo "changed" >> "$act/$f"
    expect FAIL "changed E1 file" check_exact "$old" "$act"
    restore "$f"

    f=crates/umbra-cli/Cargo.toml
    sed -i '/^\[\[bin\]\]/,$d' "$act/$f"
    expect FAIL "missing M1 hunk" check_tree "$exp" "$act"
    restore "$f"

    mkdir -p "$act/crates/cgf-planted"
    touch "$act/crates/cgf-planted/x"
    expect FAIL "path containing cgf" check_leftover "$act"
    rm -rf "${act:?}/crates/cgf-planted"
    expect PASS "restored tree, tree" check_tree "$exp" "$act"
    expect PASS "restored tree, leftover scan" check_leftover "$act"
    exit "$st"
fi

set -- $pos
[ $# -eq 2 ] || { echo "usage: rename-check.sh [--keep] [--allow PATH]... OLD NEW | --self-test"; exit 2; }
old_c=$(git rev-parse --verify -q "$1^{commit}") || { echo "FAIL: unknown revision $1"; exit 1; }
new_c=$(git rev-parse --verify -q "$2^{commit}") || { echo "FAIL: unknown revision $2"; exit 1; }
status=0
mkdir -p "$work"
git worktree prune
for t in expected old new; do rm -rf "${work:?}/${t:?}"; done
git worktree prune

git worktree add --detach "$work/old" "$old_c" >/dev/null
git worktree add --detach "$work/expected" "$old_c" >/dev/null
# Tree-level checks always read a clean worktree of NEW (no untracked files). Builds reuse the repository
# and its warm target directory when HEAD is NEW and the tree is clean.
git worktree add --detach "$work/new" "$new_c" >/dev/null
treedir="$work/new"
if [ "$(git rev-parse HEAD)" = "$new_c" ] && [ -z "$(git status --porcelain)" ]; then
    newdir="$root"; new_target="$root/target"
else
    newdir="$work/new"; new_target="$work/new-target"
fi
old_target="$work/old-target"

echo "== 1. expected tree from $1"
apply_rename "$work/expected" || { echo "FAIL: could not build the expected tree"; exit 1; }
echo "== 2. expected tree vs $2"
if check_tree "$work/expected" "$treedir"; then echo ok; else status=1; fi
echo "== 3. E1-E5 byte-identical"
if check_exact "$work/old" "$treedir"; then echo ok; else status=1; fi
echo "== 4. leftover scan"
check_leftover "$treedir" || status=1

echo "== 5. test lists equal after mapping names"
tlist() { # tlist DIR TARGET OUT [ARGS...]
    _d=$1; _t=$2; _o=$3; shift 3
    (cd "$_d" && CARGO_TARGET_DIR="$_t" cargo test --workspace --locked -- --list "$@") > "$_o.raw" 2>&1 \
        || { echo "FAIL: cargo test --list failed in $_d"; tail -n 20 "$_o.raw"; return 1; }
    awk '
        /^ *(Running|Doc-tests) / {
            if ($1 == "Doc-tests") { bin = "doctest:" $2 } else {
                bin = $0; sub(/^ *Running .*deps\//, "", bin); sub(/-[0-9a-f]+\)$/, "", bin) }
            next }
        /: test$/ { n = $0; sub(/: test$/, "", n); sub(/ \(line [0-9]+\)$/, "", n); print bin "::" n }
        /: benchmark$/ { n = $0; sub(/: benchmark$/, "", n); print bin "::bench::" n }' "$_o.raw" | sort > "$_o"
}
tlist "$work/old" "$old_target" "$work/old-tests.txt" || status=1
tlist "$work/old" "$old_target" "$work/old-ignored.txt" --ignored || status=1
tlist "$newdir" "$new_target" "$work/new-tests.txt" || status=1
tlist "$newdir" "$new_target" "$work/new-ignored.txt" --ignored || status=1
for k in tests ignored; do
    sed -E -e 's/^cgf_cli::/umbra::/' -e 's/^doctest:cgf_cli$/doctest:umbra_cli/' -e 's/cgf-/umbra-/g' -e 's/cgf_/umbra_/g' \
        "$work/old-$k.txt" | sort > "$work/old-$k.mapped"
    if cmp -s "$work/old-$k.mapped" "$work/new-$k.txt"; then
        echo "ok: $k lists equal ($(wc -l < "$work/new-$k.txt") entries)"
    else
        echo "FAIL: $k lists differ:"; diff "$work/old-$k.mapped" "$work/new-$k.txt" | head -n 30; status=1
    fi
done

echo "== 6. Cargo.lock"
lockinfo() { awk '
    /^\[\[package\]\]/ { if (n != "" && s != "") print n " " v " " s " " c; n = ""; v = ""; s = ""; c = ""; cnt++; next }
    /^name = / { n = $3 } /^version = / { v = $3 } /^source = / { s = $3 } /^checksum = / { c = $3 }
    END { if (n != "" && s != "") print n " " v " " s " " c; print "COUNT " cnt > "/dev/stderr" }' "$1/Cargo.lock"; }
lockinfo "$work/old" 2> "$work/old-count" | sort > "$work/old-ext.txt"
lockinfo "$treedir" 2> "$work/new-count" | sort > "$work/new-ext.txt"
if cmp -s "$work/old-ext.txt" "$work/new-ext.txt" && cmp -s "$work/old-count" "$work/new-count"; then
    echo "ok: $(wc -l < "$work/new-ext.txt") external entries identical; $(cat "$work/new-count")"
else
    echo "FAIL: Cargo.lock external entries or package count differ"; status=1
fi

echo "== 7. behavior"
(cd "$work/old" && CARGO_TARGET_DIR="$old_target" cargo build --locked -p cgf-cli) >/dev/null 2>&1 || { echo "FAIL: old cgf-cli build"; status=1; }
(cd "$newdir" && CARGO_TARGET_DIR="$new_target" cargo build --locked -p umbra-cli) >/dev/null 2>&1 || { echo "FAIL: umbra build"; status=1; }
oldbin="$old_target/debug/cgf-cli"; newbin="$new_target/debug/umbra"
[ -x "$oldbin" ] && [ -x "$newbin" ] || { echo "FAIL: binaries missing ($oldbin, $newbin)"; exit 1; }
# cmd_pair NAME RAW|MAP ARGS...: run old and new in their trees; compare stdout and exit status.
cmd_pair() {
    _n=$1; _m=$2; shift 2
    _args=$(printf '%s\n' "$*" | rename_text)
    set +e
    (cd "$work/old" && "$oldbin" "$@" > "$work/o-cmd.out" 2>/dev/null; echo $? > "$work/o-cmd.rc")
    # shellcheck disable=SC2086
    (cd "$newdir" && "$newbin" $_args > "$work/n-cmd.out" 2>/dev/null; echo $? > "$work/n-cmd.rc")
    set -e
    if [ "$_m" = MAP ]; then rename_text < "$work/o-cmd.out" > "$work/o-cmd.cmp"; else cp "$work/o-cmd.out" "$work/o-cmd.cmp"; fi
    if cmp -s "$work/o-cmd.cmp" "$work/n-cmd.out" && cmp -s "$work/o-cmd.rc" "$work/n-cmd.rc"; then
        echo "ok: $_n (exit $(cat "$work/n-cmd.rc"), $(wc -c < "$work/n-cmd.out") bytes)"
    else
        echo "FAIL: $_n differs"; diff "$work/o-cmd.cmp" "$work/n-cmd.out" | head -n 10; status=1
    fi
}
for s in walking_skeleton patrol_and_engage; do
    cmd_pair "run-$s" RAW run "examples/scenarios/$s/scenario.toml" --hash-every 1
    echo "   last line: $(tail -n 1 "$work/n-cmd.out")"
done
cmd_pair validate-json MAP validate examples/scenarios --json
cmd_pair validate-bad MAP validate crates/cgf-cli/tests/fixtures/bad --json
cmd_pair validate-text MAP validate examples/scenarios crates/cgf-cli/tests/fixtures/bad
for k in template scenario behavior; do cmd_pair "schema-$k" MAP schema "$k"; done
ids=$(cd "$work/old" && grep -rhoE 'builtin\.[a-z0-9_.]+' crates/*/src crates/*/schemas | sed -E 's/\.(json|behavior\.toml)$//; s/\.$//' | sort -u)
for id in $ids; do cmd_pair "component-$id" RAW schema component "$id"; done
echo "   component ids checked: $(echo "$ids" | wc -w)"

echo "== 8. check-deps parity"
xt() { # xt DIR TARGET ARGS: output of cargo xtask check-deps, exit status as the last line
    _d=$1; _t=$2; shift 2
    set +e
    (cd "$_d" && CARGO_TARGET_DIR="$_t" cargo xtask check-deps "$@" 2>&1; echo "exit=$?") \
        | grep -vE '^ *(Compiling|Finished|Running|Locking|Updating|Downloaded|Fresh) ' | sed "s#$_d#<TREE>#g"
    set -e
}
for fx in "" violating conforming; do
    if [ -n "$fx" ]; then a="--manifest-path xtask/tests/fixtures/$fx/Cargo.toml"; else a=""; fi
    # shellcheck disable=SC2086
    xt "$work/old" "$old_target" $a | rename_text | sort > "$work/o-deps.txt"
    # shellcheck disable=SC2086
    xt "$newdir" "$new_target" $(printf '%s\n' "$a" | rename_text) | sort > "$work/n-deps.txt"
    if cmp -s "$work/o-deps.txt" "$work/n-deps.txt"; then
        echo "ok: check-deps ${fx:-workspace} ($(grep -c . "$work/n-deps.txt") lines, $(grep '^exit=' "$work/n-deps.txt"))"
    else
        echo "FAIL: check-deps ${fx:-workspace} differs"; diff "$work/o-deps.txt" "$work/n-deps.txt" | head -n 20; status=1
    fi
done

if [ "$status" -eq 0 ]; then echo "RENAME CHECK PASSED"; else echo "RENAME CHECK FAILED"; fi
exit "$status"
