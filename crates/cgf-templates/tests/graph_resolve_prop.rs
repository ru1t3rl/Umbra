//! AT-8 (property): random forests of behavior graphs. Cycles and depth are reported exactly when
//! the reference graph says so, broken graphs cause no cascade, and the order of the files does not
//! matter.
//! Requirements: REQ-B-12, REQ-T-2, REQ-K-4, ADR 0012, ASM-056, ASM-073.
//!
//! Step 4 of M3: written before WI-13 and enabled with it. The acceptance check runs it at 1000
//! cases.

#![forbid(unsafe_code)]

mod common;
mod graph_support;

use std::collections::BTreeSet;

use cgf_templates::{Diagnostic, SourceFile};
use common::{lines, src};
use graph_support::graphs_with_depth;
use proptest::prelude::*;

/// A forest: `edges[i]` are the graphs that graph `i` calls; `limit` is the depth limit; `order`
/// is a permutation of the graph indices (the order the files are given in).
#[derive(Debug, Clone)]
struct Forest {
    edges: Vec<BTreeSet<usize>>,
    limit: u32,
    order: Vec<usize>,
}

fn forest() -> impl Strategy<Value = Forest> {
    (1usize..=30).prop_flat_map(|n| {
        (
            any::<bool>(),
            prop::collection::vec(prop::collection::btree_set(0..n, 0..4), n),
            2u32..7,
            Just((0..n).collect::<Vec<_>>()).prop_shuffle(),
        )
            .prop_map(|(acyclic, mut edges, limit, order)| {
                if acyclic {
                    for (i, targets) in edges.iter_mut().enumerate() {
                        targets.retain(|&j| j > i);
                    }
                }
                Forest {
                    edges,
                    limit,
                    order,
                }
            })
    })
}

fn id(i: usize) -> String {
    format!("test.g{i:02}")
}

fn graph_file(forest: &Forest, i: usize, broken: bool) -> SourceFile {
    let targets: Vec<usize> = forest.edges[i].iter().copied().collect();
    let mut all = vec![
        "schema_version = 1".to_string(),
        format!("id = \"{}\"", id(i)),
        "version = \"1.0.0\"".to_string(),
        "root = \"r\"".to_string(),
        "[nodes.r]".to_string(),
    ];
    if targets.is_empty() {
        all.push("type = \"action\"".into());
        all.push("ref = \"stub.action.noop\"".into());
    } else {
        all.push("type = \"sequence\"".into());
        let children: Vec<String> = (0..targets.len()).map(|k| format!("\"c{k}\"")).collect();
        all.push(format!("children = [{}]", children.join(", ")));
        for (k, t) in targets.iter().enumerate() {
            all.push(format!("[nodes.c{k}]"));
            all.push("type = \"subgraph\"".into());
            all.push(format!("ref = \"{}\"", id(*t)));
        }
    }
    if broken {
        all.push("this is not toml".into());
    }
    let refs: Vec<&str> = all.iter().map(String::as_str).collect();
    src(&format!("behaviors/g{i:02}.behavior.toml"), &lines(&refs))
}

fn load(forest: &Forest, broken: &BTreeSet<usize>) -> Vec<Diagnostic> {
    let files: Vec<SourceFile> = forest
        .order
        .iter()
        .map(|&i| graph_file(forest, i, broken.contains(&i)))
        .collect();
    match graphs_with_depth(&files, forest.limit) {
        Ok(_) => Vec::new(),
        Err(d) => d.iter().cloned().collect(),
    }
}

/// Strongly connected components with a cycle: more than one graph, or one with a self-call.
fn cyclic_components(edges: &[BTreeSet<usize>]) -> usize {
    struct State<'a> {
        edges: &'a [BTreeSet<usize>],
        index: Vec<Option<usize>>,
        low: Vec<usize>,
        on_stack: Vec<bool>,
        stack: Vec<usize>,
        next: usize,
        cyclic: usize,
    }
    fn visit(s: &mut State<'_>, v: usize) {
        s.index[v] = Some(s.next);
        s.low[v] = s.next;
        s.next += 1;
        s.stack.push(v);
        s.on_stack[v] = true;
        for &w in &s.edges[v] {
            match s.index[w] {
                None => {
                    visit(s, w);
                    s.low[v] = s.low[v].min(s.low[w]);
                }
                Some(iw) if s.on_stack[w] => s.low[v] = s.low[v].min(iw),
                Some(_) => {}
            }
        }
        if Some(s.low[v]) == s.index[v] {
            let mut size = 0;
            while let Some(w) = s.stack.pop() {
                s.on_stack[w] = false;
                size += 1;
                if w == v {
                    break;
                }
            }
            if size > 1 || s.edges[v].contains(&v) {
                s.cyclic += 1;
            }
        }
    }
    let n = edges.len();
    let mut s = State {
        edges,
        index: vec![None; n],
        low: vec![0; n],
        on_stack: vec![false; n],
        stack: Vec::new(),
        next: 0,
        cyclic: 0,
    };
    for v in 0..n {
        if s.index[v].is_none() {
            visit(&mut s, v);
        }
    }
    s.cyclic
}

/// Number of graphs on the longest chain of an acyclic forest (a graph with no calls counts 1).
///
/// WI-13 test fix: the first version assumed every edge points to a higher index, which holds only
/// for the forests generated as "acyclic" by construction; a forest generated freely can also have
/// no cycle (`2 -> 1 -> 0`), and its longest chain was undercounted. This memoised walk is correct
/// for any acyclic forest.
fn longest_chain(edges: &[BTreeSet<usize>]) -> usize {
    fn chain(edges: &[BTreeSet<usize>], v: usize, memo: &mut Vec<Option<usize>>) -> usize {
        if let Some(found) = memo[v] {
            return found;
        }
        let best = 1 + edges[v]
            .iter()
            .map(|&w| chain(edges, w, memo))
            .max()
            .unwrap_or(0);
        memo[v] = Some(best);
        best
    }
    let mut memo = vec![None; edges.len()];
    (0..edges.len())
        .map(|v| chain(edges, v, &mut memo))
        .max()
        .unwrap_or(0)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    /// REQ-B-12: loading terminates; `subgraph_cycle` appears once per cycle (a strongly connected
    /// component with a loop); in an acyclic forest `subgraph_too_deep` appears exactly when the
    /// longest chain is longer than the limit; no other code appears (no cascade from a cycle);
    /// and the same files in another order give identical diagnostics.
    #[test]
    fn subgraph_cycles_and_depth(forest in forest()) {
        let diagnostics = load(&forest, &BTreeSet::new());

        let cycles = diagnostics.iter().filter(|d| d.code.as_str() == "subgraph_cycle").count();
        prop_assert_eq!(cycles, cyclic_components(&forest.edges));
        let too_deep = diagnostics.iter().any(|d| d.code.as_str() == "subgraph_too_deep");
        if cyclic_components(&forest.edges) == 0 {
            let limit = usize::try_from(forest.limit).unwrap_or(usize::MAX);
            prop_assert_eq!(too_deep, longest_chain(&forest.edges) > limit);
        }
        for d in &diagnostics {
            prop_assert!(
                matches!(d.code.as_str(), "subgraph_cycle" | "subgraph_too_deep"),
                "unexpected {}",
                d
            );
        }
        let mut reordered = forest.clone();
        reordered.order.reverse();
        prop_assert_eq!(load(&reordered, &BTreeSet::new()), diagnostics);
    }

    /// ASM-056: graphs in files with a syntax error are "broken"; the graphs that call them get no
    /// `unknown_id` or other cascade, whatever the shape of the forest.
    #[test]
    fn broken_graphs_cause_no_cascade(forest in forest(), pick in prop::collection::btree_set(0usize..30, 0..6)) {
        let broken: BTreeSet<usize> = pick.into_iter().filter(|&i| i < forest.edges.len()).collect();

        let diagnostics = load(&forest, &broken);

        for d in &diagnostics {
            prop_assert!(
                matches!(d.code.as_str(), "syntax" | "subgraph_cycle" | "subgraph_too_deep"),
                "unexpected {}",
                d
            );
        }
        let syntax = diagnostics.iter().filter(|d| d.code.as_str() == "syntax").count();
        prop_assert_eq!(syntax, broken.len(), "one syntax error per broken file");
    }
}
