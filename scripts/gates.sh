#!/bin/sh
# The local gate list of docs/build.md (canonical, ADR 0004 / D-19) with compact output: one line per
# gate, the full log of each gate in target/gates/<name>.log, and the tail of the log of a failing gate.
# Stops at the first failure, as docs/build.md requires. Needs the prerequisites of docs/build.md.
#
#   scripts/gates.sh                 every active gate, then m2-compat and the golden state hashes
#   scripts/gates.sh --quick CRATE…  fmt, clippy and tests of the named crates only (engineers, during a WI)
#   scripts/gates.sh --gui           also gate 15 (GUI, from M5b WI-1; needs Bun and Node LTS)
#
# Gate 10 runs once `cargo xtask gen-api` exists (M5); gates 3 and 11 get target/tools/bin on PATH
# (pinned cbindgen, until it is installed system-wide).
set -u
cd "$(dirname "$0")/.." || exit 2
PATH="$PWD/target/tools/bin:$PATH"
export PATH
LOGS=target/gates
mkdir -p "$LOGS"

# Golden final state hashes (docs/milestones.md): name, expected hash, extra `umbra run` arguments.
GOLDENS='walking_skeleton 849026656b7b42d2 -
patrol_and_engage 3a7bfda0a76111b9 -
plugin_wander 88954c5dc6b52b48 --plugin-path=target/plugins
plugin_c_circle 3cc077a4c83c4a63 --plugin-path=target/plugins'

# gate NAME COMMAND…: runs COMMAND with its output in $LOGS/NAME.log; prints one line; exits on failure.
gate() {
    name=$1
    shift
    start=$(date +%s)
    if "$@" >"$LOGS/$name.log" 2>&1; then
        printf 'ok    %-14s %4ss%s\n' "$name" "$(($(date +%s) - start))" "$(summary "$name")"
    else
        printf 'FAIL  %-14s %4ss  (log: %s)\n' "$name" "$(($(date +%s) - start))" "$LOGS/$name.log"
        tail -n 40 "$LOGS/$name.log"
        exit 1
    fi
}

# summary NAME: a short suffix for test gates (passed/failed/ignored totals).
summary() {
    grep -E '^test result' "$LOGS/$1.log" 2>/dev/null |
        awk '{p+=$4; f+=$6; i+=$8; n++} END {if (n) printf "  %d passed, %d failed, %d ignored", p, f, i}'
}

goldens() {
    rc=0
    echo "$GOLDENS" | while read -r scenario expected extra; do
        [ "$extra" = - ] && extra=
        # shellcheck disable=SC2086 # $extra is one optional flag
        got=$(target/release/umbra run "examples/scenarios/$scenario/scenario.toml" $extra 2>/dev/null |
            grep -o 'state_hash=[0-9a-f]*' | tail -n 1)
        if [ "$got" = "state_hash=$expected" ]; then
            echo "ok   $scenario $expected"
        else
            echo "FAIL $scenario expected $expected, got ${got:-nothing}"
            exit 1
        fi
    done || rc=1
    return $rc
}

if [ "${1:-}" = --quick ]; then
    shift
    [ $# -gt 0 ] || { echo "usage: scripts/gates.sh --quick CRATE..." >&2; exit 2; }
    pkgs=
    for c in "$@"; do pkgs="$pkgs -p $c"; done
    gate fmt cargo fmt --all -- --check
    # shellcheck disable=SC2086 # $pkgs is a list of -p flags
    gate clippy cargo clippy $pkgs --all-targets --locked --offline -- -D warnings
    # shellcheck disable=SC2086
    gate test cargo test $pkgs --locked --offline
    echo "quick gates passed (not the definition of done: run scripts/gates.sh for that)"
    exit 0
fi

gate 1-fmt cargo fmt --all -- --check
gate 2-clippy cargo clippy --workspace --all-targets --locked --offline -- -D warnings
gate 3-test cargo test --workspace --locked --offline
gate 4-determinism cargo test -p umbra-engine --test determinism --test behavior_determinism \
    --test plugin_determinism --test handle_determinism --test net_determinism --locked --offline
gate 5-deps cargo xtask check-deps
gate 6-deny cargo deny check
gate 7-audit cargo audit
gate 8-doc env RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked --offline
gate 9-release cargo build --workspace --release --locked --offline
if grep -q 'gen-api' xtask/src/*.rs 2>/dev/null; then
    gate 10-schema cargo xtask gen-api --check
else
    echo "skip  10-schema         (cargo xtask gen-api does not exist yet)"
fi
gate 11-header cargo xtask gen-header --check
gate 12-plugins cargo xtask build-plugins --release
gate 13-plugin-fail cargo test -p umbra-plugin-host --test plugin_failures --locked --offline
gate 14-bench cargo bench -p umbra-kernel --bench tick --no-run --locked --offline
gate 16-engine-only sh -c 'cargo build -p umbra-cli --no-default-features --locked --offline &&
    cargo clippy -p umbra-cli --no-default-features --all-targets --locked --offline -- -D warnings'
if [ "${1:-}" = --gui ]; then
    gate 15-gui sh -c 'cd gui && bun install --frozen-lockfile && bun run verify'
else
    echo "skip  15-gui            (pass --gui)"
fi
gate m2-compat sh scripts/m2-compat.sh
gate goldens goldens
grep -E '^(ok|FAIL)' "$LOGS/goldens.log"
echo "all gates passed"
