//! AT-3 (property): over random trees of stub leaves no node is ticked twice in one tick, and two
//! runs of the same tree give identical statuses, traces and intents (REQ-K-4).
//! Requirements: REQ-B-8, REQ-B-16, ADR 0013, ASM-070.
//!
//! Step 4 of M3: written before WI-8; un-ignored in WI-8.

#![forbid(unsafe_code)]

mod common;

use std::collections::BTreeMap;

use cgf_core::{NodeId, ParallelPolicy};
use common::{B, Bench, Step};
use proptest::prelude::*;

/// A random tree. Leaves are scripted actions (`0` success, `1` failure, `2` running) and flag
/// conditions with a fixed value.
#[derive(Debug, Clone)]
enum Spec {
    Act(Vec<u8>),
    Cond(bool),
    Seq(bool, Vec<Spec>),
    Sel(bool, Vec<Spec>),
    Par(bool, Vec<Spec>),
    Inverter(Box<Spec>),
    SucceedAlways(Box<Spec>),
    Repeat(Option<u32>, Box<Spec>),
    /// Cooldown of 0.1 s (3 ticks at 30 Hz).
    Cooldown(Box<Spec>),
    /// Timeout of 0.1 s (3 ticks at 30 Hz).
    Timeout(Box<Spec>),
}

fn spec() -> impl Strategy<Value = Spec> {
    let leaf = prop_oneof![
        prop::collection::vec(0u8..3, 1..5).prop_map(Spec::Act),
        any::<bool>().prop_map(Spec::Cond),
    ];
    leaf.prop_recursive(4, 32, 4, |inner| {
        prop_oneof![
            (any::<bool>(), prop::collection::vec(inner.clone(), 1..4))
                .prop_map(|(m, c)| Spec::Seq(m, c)),
            (any::<bool>(), prop::collection::vec(inner.clone(), 1..4))
                .prop_map(|(m, c)| Spec::Sel(m, c)),
            (any::<bool>(), prop::collection::vec(inner.clone(), 1..4))
                .prop_map(|(all, c)| Spec::Par(all, c)),
            inner.clone().prop_map(|c| Spec::Inverter(Box::new(c))),
            inner.clone().prop_map(|c| Spec::SucceedAlways(Box::new(c))),
            (prop::option::of(1u32..4), inner.clone())
                .prop_map(|(n, c)| Spec::Repeat(n, Box::new(c))),
            inner.clone().prop_map(|c| Spec::Cooldown(Box::new(c))),
            inner.prop_map(|c| Spec::Timeout(Box::new(c))),
        ]
    })
}

fn build(b: &mut B, spec: &Spec, flags: &mut BTreeMap<String, bool>) -> NodeId {
    let n = flags.len();
    match spec {
        Spec::Act(script) => {
            let words: Vec<&str> = script
                .iter()
                .map(|s| ["success", "failure", "running"][usize::from(*s)])
                .collect();
            // Reserve a name slot so every node gets a unique counter value.
            flags.insert(format!("n{n}"), false);
            b.act(&format!("a{n}"), &words)
        }
        Spec::Cond(value) => {
            let name = format!("c{n}");
            flags.insert(name.clone(), *value);
            b.cond(&name)
        }
        Spec::Seq(memory, children) | Spec::Sel(memory, children) => {
            // Reserve a name slot so the counter advances for composites too.
            flags.insert(format!("n{n}"), false);
            let ids: Vec<NodeId> = children.iter().map(|c| build(b, c, flags)).collect();
            if matches!(spec, Spec::Seq(..)) {
                b.seq(&format!("s{n}"), *memory, &ids)
            } else {
                b.sel(&format!("s{n}"), *memory, &ids)
            }
        }
        Spec::Par(all, children) => {
            flags.insert(format!("n{n}"), false);
            let ids: Vec<NodeId> = children.iter().map(|c| build(b, c, flags)).collect();
            let policy = if *all {
                ParallelPolicy::All
            } else {
                ParallelPolicy::Any
            };
            b.par(&format!("p{n}"), policy, &ids)
        }
        Spec::Inverter(child)
        | Spec::SucceedAlways(child)
        | Spec::Repeat(_, child)
        | Spec::Cooldown(child)
        | Spec::Timeout(child) => {
            flags.insert(format!("n{n}"), false);
            let id = build(b, child, flags);
            match spec {
                Spec::Inverter(_) => b.inverter(&format!("i{n}"), id),
                Spec::SucceedAlways(_) => b.succeed_always(&format!("w{n}"), id),
                Spec::Repeat(count, _) => b.repeat(&format!("r{n}"), id, *count),
                Spec::Cooldown(_) => b.cooldown(&format!("k{n}"), id, 0.1),
                Spec::Timeout(_) => b.timeout(&format!("t{n}"), id, 0.1),
                _ => unreachable!("matched above"),
            }
        }
    }
}

fn run(spec: &Spec, ticks: usize) -> Vec<Step> {
    let bench = Bench::new();
    let mut b = B::new();
    let mut flags = BTreeMap::new();
    let root = build(&mut b, spec, &mut flags);
    for (name, value) in &flags {
        bench.controls.set_flag(name, *value);
    }
    bench.run(b.graph("test.graph.random", root)).steps(ticks)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// REQ-B-8: a leaf is evaluated at most once per tick; REQ-K-4: the same tree gives the same
    /// statuses, traces and intents on a second run.
    #[test]
    fn no_node_ticks_twice_and_trace_is_reproducible(spec in spec()) {
        let first = run(&spec, 24);
        for (tick, step) in first.iter().enumerate() {
            let mut seen = BTreeMap::new();
            for line in step.trace.iter().filter(|l| !l.starts_with("halt:")) {
                *seen.entry(line.clone()).or_insert(0usize) += 1;
            }
            for (line, n) in seen {
                prop_assert_eq!(n, 1, "{} was evaluated {} times in tick {}", line, n, tick);
            }
        }
        prop_assert_eq!(first, run(&spec, 24));
    }
}
