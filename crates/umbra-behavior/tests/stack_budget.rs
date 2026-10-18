//! Stack budget of the recursive tick (plan WI-10 Risks, ASM-098): the worst case the loader and
//! the interpreter allow is 16 graph levels of 128 node levels each. This test builds it and
//! ticks, halts and drops it on a thread with a fixed stack. Requirements: REQ-B-12 (nesting limits), ASM-098.

#![forbid(unsafe_code)]

mod common;

use common::{B, Bench, call};
use serde_json::json;
use umbra_core::{FsmDef, NodeId, ParallelPolicy, StateDef};

/// Graph levels (ASM-073) and node levels per graph (`MAX_NODE_DEPTH`).
const GRAPHS: usize = 16;
const NODE_LEVELS: usize = 128;

/// The stack the worst case is run on. Debug builds use the most.
fn stack_kib() -> usize {
    std::env::var("UMBRA_STACK_KIB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_STACK_KIB)
}

const DEFAULT_STACK_KIB: usize = 2048;

/// The node types that wrap one child, one per frame shape of the walk.
const WRAPPERS: [&str; 7] = [
    "inverter", "sequence", "parallel", "repeat", "timeout", "cooldown", "fsm",
];

fn wrap(b: &mut B, kind: &str, name: &str, child: NodeId) -> NodeId {
    match kind {
        "inverter" => b.inverter(name, child),
        "sequence" => b.seq(name, true, &[child]),
        "parallel" => b.par(name, ParallelPolicy::All, &[child]),
        "repeat" => b.repeat(name, child, None),
        "timeout" => b.timeout(name, child, 1e6),
        "cooldown" => b.cooldown(name, child, 1.0),
        _ => b.fsm(name, FsmDef::new(0, vec![StateDef::new("s", child)])),
    }
}

/// Graph `i` is 127 nested `kind` nodes over a `subgraph` node calling graph `i + 1`; the last
/// graph has an action at the bottom instead.
fn chain(bench: &mut Bench, kind: &str) {
    let mut graphs = Vec::new();
    for level in 0..GRAPHS {
        let mut b = B::new();
        let mut node = if level + 1 == GRAPHS {
            b.act("leaf", &["running"])
        } else {
            b.subgraph(
                "call",
                call(&format!("test.deep.g{:02}", level + 1), json!({})),
            )
        };
        for i in 0..NODE_LEVELS - 1 {
            node = wrap(&mut b, kind, &format!("n{i}"), node);
        }
        graphs.push(b.graph(&format!("test.deep.g{level:02}"), node));
    }
    bench.register(graphs);
}

/// 16 x 128 nested frames tick, halt (parent and instance) and drop without overflowing the stack.
#[test]
fn worst_case_nesting_runs_within_the_stack_budget() {
    let worker = std::thread::Builder::new()
        .stack_size(stack_kib() * 1024)
        .spawn(|| {
            let mut outcomes = Vec::new();
            for kind in WRAPPERS {
                let mut bench = Bench::new();
                chain(&mut bench, kind);
                let mut run = bench.run_registered("test.deep.g00", &json!({}));
                let first = run.step();
                run.instance.halt_running();
                let second = run.step();
                run.halt();
                outcomes.push((kind, first.status, second.status, first.trace));
            }
            outcomes
        });
    let result = worker.map(std::thread::JoinHandle::join);
    let Ok(Ok(outcomes)) = result else {
        panic!("the worst case did not run: {result:?}");
    };
    for (kind, first, second, trace) in outcomes {
        assert_eq!(first, umbra_core::Status::Running, "{kind}");
        assert_eq!(second, umbra_core::Status::Running, "{kind}");
        assert_eq!(trace, vec!["tick:leaf".to_string()], "{kind}");
    }
}
