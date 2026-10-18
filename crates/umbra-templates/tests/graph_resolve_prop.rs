//! AT-8 (property): random forests of behavior graphs. Cycles, depth, versions and duplicate IDs
//! are reported exactly where an independent reference model says so, broken graphs cause no
//! cascade, and the order of the files does not matter.
//! Requirements: REQ-B-12, REQ-T-2, REQ-K-4, ADR 0012, ASM-056, ASM-073, ASM-092 (c), ASM-106.
//!
//! Step 4 of M3: written before WI-13 and enabled with it; strengthened after the WI-13 review.
//! The acceptance check runs it at 1000 cases.
//!
//! The model (`expect`) works on plain indices: graph `i` has ID `test.g{i:02}`, so ID order is
//! index order, while its file path `behaviors/g{paths[i]:02}` uses a separate random permutation,
//! so a rule that used path order instead of ID order would disagree with the model.

#![forbid(unsafe_code)]

mod common;
mod graph_support;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use common::{Row, lines, row, rows, src};
use graph_support::graphs_with_depth;
use proptest::prelude::*;
use umbra_templates::{Diagnostic, SourceFile};

/// How a graph's file is damaged, if at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Valid.
    Clean,
    /// A syntax error after the nodes; the `id` can still be recovered (ASM-054 rule 3).
    Junk,
    /// `version = 1`: the file parses but the reader reports an error (ASM-056, ASM-106 a).
    ReaderError,
}

/// A call: the target index and the `version` range of the call, `^m.0.0` for `Some(m)`.
type Calls = BTreeMap<usize, Option<u8>>;

/// A forest of graphs. `calls[i]` are the graphs graph `i` calls, in target order (node `c{k}` is
/// the `k`-th target); `majors[i]` is the major version of graph `i`; `keys` order the files
/// (graph files first, then one duplicate file per entry of `dups`).
#[derive(Debug, Clone)]
struct Forest {
    calls: Vec<Calls>,
    majors: Vec<u8>,
    limit: u32,
    keys: Vec<u32>,
    paths: Vec<usize>,
    modes: Vec<Mode>,
    dups: BTreeSet<usize>,
}

/// Most files: 30 graphs and 6 duplicate files.
const MAX_FILES: usize = 36;

fn forest(damaged: bool) -> impl Strategy<Value = Forest> {
    (1usize..=30).prop_flat_map(move |n| {
        let mode = prop_oneof![8 => Just(Mode::Clean), 1 => Just(Mode::Junk), 1 => Just(Mode::ReaderError)];
        (
            (any::<bool>(), any::<bool>()),
            prop::collection::vec(
                prop::collection::btree_map(0..n, prop::option::of(1u8..=4), 0..4),
                n,
            ),
            prop::collection::vec(1u8..=3, n),
            2u32..7,
            prop::collection::vec(any::<u32>(), MAX_FILES),
            Just((0..n).collect::<Vec<_>>()).prop_shuffle(),
            prop::collection::vec(mode, n),
            prop::collection::btree_set(0..n, 0..6),
        )
            .prop_map(
                move |((acyclic, local), calls, majors, limit, keys, paths, modes, dups)| {
                    let calls = calls
                        .into_iter()
                        .enumerate()
                        .map(|(i, targets)| shape(i, n, targets, acyclic, local))
                        .collect();
                    Forest {
                        calls,
                        majors,
                        limit,
                        keys,
                        paths,
                        modes: if damaged { modes } else { vec![Mode::Clean; n] },
                        dups: if damaged { dups } else { BTreeSet::new() },
                    }
                },
            )
    })
}

/// Makes the random targets of graph `i` forward-only (`acyclic`) and/or close to `i` (`local`,
/// which makes long chains likely).
fn shape(i: usize, n: usize, targets: Calls, acyclic: bool, local: bool) -> Calls {
    let mut out = Calls::new();
    for (t, range) in targets {
        let t = if local { (i + 1 + t % 3).min(n - 1) } else { t };
        if !acyclic || t > i {
            out.insert(t, range);
        }
    }
    out
}

fn id(i: usize) -> String {
    format!("test.g{i:02}")
}

fn graph_path(f: &Forest, i: usize) -> PathBuf {
    PathBuf::from(format!("behaviors/g{:02}.behavior.toml", f.paths[i]))
}

fn dup_path(j: usize) -> PathBuf {
    PathBuf::from(format!("behaviors/zdup{j:02}.behavior.toml"))
}

/// Where the parts of a graph file are: line of node `c{k}`'s `ref` and `version`.
struct Lines {
    refs: Vec<u32>,
    versions: Vec<Option<u32>>,
}

fn to_line(index: usize) -> u32 {
    u32::try_from(index + 1).unwrap_or(u32::MAX)
}

fn graph_text(f: &Forest, i: usize) -> (String, Lines) {
    let version = match f.modes[i] {
        Mode::ReaderError => "version = 1".to_string(),
        _ => format!("version = \"{}.0.0\"", f.majors[i]),
    };
    let mut all = vec![
        "schema_version = 1".to_string(),
        format!("id = \"{}\"", id(i)),
        version,
        "root = \"r\"".to_string(),
        "[nodes.r]".to_string(),
    ];
    let mut at = Lines {
        refs: Vec::new(),
        versions: Vec::new(),
    };
    if f.calls[i].is_empty() {
        all.push("type = \"action\"".into());
        all.push("ref = \"stub.action.noop\"".into());
    } else {
        all.push("type = \"sequence\"".into());
        let children: Vec<String> = (0..f.calls[i].len()).map(|k| format!("\"c{k}\"")).collect();
        all.push(format!("children = [{}]", children.join(", ")));
        for (k, (t, range)) in f.calls[i].iter().enumerate() {
            all.push(format!("[nodes.c{k}]"));
            all.push("type = \"subgraph\"".into());
            all.push(format!("ref = \"{}\"", id(*t)));
            at.refs.push(to_line(all.len() - 1));
            at.versions.push(range.map(|m| {
                all.push(format!("version = \"^{m}.0.0\""));
                to_line(all.len() - 1)
            }));
        }
    }
    if f.modes[i] == Mode::Junk {
        all.push("this is not toml".into());
    }
    let refs: Vec<&str> = all.iter().map(String::as_str).collect();
    (lines(&refs), at)
}

/// The files in the order given to the loader: graph files, then the duplicates.
fn files(f: &Forest, reversed: bool) -> Vec<SourceFile> {
    let mut all: Vec<SourceFile> = (0..f.calls.len())
        .map(|i| src(&graph_path(f, i).to_string_lossy(), &graph_text(f, i).0))
        .collect();
    for &j in &f.dups {
        let text = lines(&[
            "schema_version = 1",
            &format!("id = \"{}\"", id(j)),
            "version = \"1.0.0\"",
            "root = \"r\"",
            "[nodes.r]",
            "type = \"action\"",
            "ref = \"stub.action.noop\"",
        ]);
        all.push(src(&dup_path(j).to_string_lossy(), &text));
    }
    let mut order: Vec<usize> = (0..all.len()).collect();
    order.sort_by_key(|&x| (f.keys[x], x));
    if reversed {
        order.reverse();
    }
    order.into_iter().map(|x| all[x].clone()).collect()
}

fn load(f: &Forest, reversed: bool) -> Vec<Diagnostic> {
    match graphs_with_depth(&files(f, reversed), f.limit) {
        Ok(_) => Vec::new(),
        Err(d) => d.iter().cloned().collect(),
    }
}

/// The rows of every diagnostic the model predicts: `subgraph_cycle`, `subgraph_too_deep`,
/// `version_mismatch` and `duplicate_id` (the syntax and reader errors of damaged files are
/// checked by count and file).
fn expect(f: &Forest) -> Vec<Row> {
    let n = f.calls.len();
    let usable: Vec<bool> = (0..n)
        .map(|i| f.modes[i] == Mode::Clean && !f.dups.contains(&i))
        .collect();
    // Edges of the reference graph: usable graph to usable graph (ASM-106 a, f).
    let adj: Vec<Vec<usize>> = (0..n)
        .map(|i| {
            if usable[i] {
                f.calls[i].keys().copied().filter(|&t| usable[t]).collect()
            } else {
                Vec::new()
            }
        })
        .collect();
    let mut reach = vec![vec![false; n]; n];
    for (v, targets) in adj.iter().enumerate() {
        for &t in targets {
            reach[v][t] = true;
        }
    }
    for k in 0..n {
        for a in 0..n {
            for b in 0..n {
                if reach[a][k] && reach[k][b] {
                    reach[a][b] = true;
                }
            }
        }
    }
    let cyclic: Vec<bool> = (0..n).map(|v| reach[v][v]).collect();
    let mut out: Vec<Row> = Vec::new();
    let position = |v: usize, t: usize| f.calls[v].keys().position(|&x| x == t).unwrap_or(0);

    // One cycle per strongly connected component, at the first reference of the shortest cycle
    // through its smallest ID; ties go to the smallest target ID (ASM-092 c).
    for v in (0..n).filter(|&v| cyclic[v]) {
        if (0..v).any(|u| reach[v][u] && reach[u][v]) {
            continue;
        }
        let mut dist = vec![usize::MAX; n];
        for _ in 0..=n {
            for u in 0..n {
                for &t in &adj[u] {
                    let via = if t == v { 1 } else { dist[t].saturating_add(1) };
                    dist[u] = dist[u].min(via);
                }
            }
        }
        let shortest = adj[v]
            .iter()
            .map(|&w| dist_to(&dist, v, w))
            .min()
            .unwrap_or(0);
        let first = adj[v]
            .iter()
            .copied()
            .find(|&w| dist_to(&dist, v, w) == shortest)
            .unwrap_or(v);
        let line = to_line_of(f, v, position(v, first), false);
        out.push(row("subgraph_cycle", graph_path(f, v), (line, 7)));
    }

    // Depth outside the cyclic components: a reference made by a graph whose longest chain from
    // a root is exactly the limit (one per outgoing reference).
    let acyclic: Vec<usize> = (0..n).filter(|&v| usable[v] && !cyclic[v]).collect();
    let mut level = vec![1usize; n];
    for _ in 0..=n {
        for &v in &acyclic {
            for &t in adj[v].iter().filter(|&&t| !cyclic[t]) {
                level[t] = level[t].max(level[v] + 1);
            }
        }
    }
    let limit = usize::try_from(f.limit).unwrap_or(usize::MAX);
    for &v in acyclic.iter().filter(|&&v| level[v] == limit) {
        for &t in adj[v].iter().filter(|&&t| !cyclic[t]) {
            let line = to_line_of(f, v, position(v, t), false);
            out.push(row("subgraph_too_deep", graph_path(f, v), (line, 7)));
        }
    }

    // A call with a range no usable target satisfies, from any file that parsed.
    for v in (0..n).filter(|&v| f.modes[v] != Mode::Junk) {
        for (k, (&t, range)) in f.calls[v].iter().enumerate() {
            if let Some(m) = range
                && usable[t]
                && *m != f.majors[t]
            {
                let line = to_line_of(f, v, k, true);
                out.push(row("version_mismatch", graph_path(f, v), (line, 11)));
            }
        }
    }

    // The later file of a duplicated ID (the duplicate files sort last).
    for &j in &f.dups {
        out.push(row("duplicate_id", dup_path(j), (2, 6)));
    }
    out.sort();
    out
}

/// Length of the shortest path from `w` to `v` plus the edge `v -> w`, as a cycle length.
fn dist_to(dist: &[usize], v: usize, w: usize) -> usize {
    if w == v { 1 } else { dist[w].saturating_add(1) }
}

/// The line of the `ref` (or the `version`) of the `k`-th call of graph `v`.
fn to_line_of(f: &Forest, v: usize, k: usize, version: bool) -> u32 {
    let at = graph_text(f, v).1;
    if version {
        at.versions[k].unwrap_or(0)
    } else {
        at.refs[k]
    }
}

const MODELLED: [&str; 4] = [
    "subgraph_cycle",
    "subgraph_too_deep",
    "version_mismatch",
    "duplicate_id",
];

fn check(f: &Forest) -> Result<(), TestCaseError> {
    let diagnostics = load(f, false);
    let (modelled, other): (Vec<Diagnostic>, Vec<Diagnostic>) = diagnostics
        .iter()
        .cloned()
        .partition(|d| MODELLED.contains(&d.code.as_str()));
    let mut got = rows(&modelled);
    got.sort();
    prop_assert_eq!(got, expect(f));

    // Every other diagnostic is the one error of a damaged file, and nothing else (ASM-056).
    let junk: BTreeSet<PathBuf> = (0..f.calls.len())
        .filter(|&i| f.modes[i] == Mode::Junk)
        .map(|i| graph_path(f, i))
        .collect();
    let reader: BTreeSet<PathBuf> = (0..f.calls.len())
        .filter(|&i| f.modes[i] == Mode::ReaderError)
        .map(|i| graph_path(f, i))
        .collect();
    let mut syntax_in = BTreeSet::new();
    let mut reader_in = BTreeSet::new();
    for d in &other {
        let path = d
            .location
            .as_ref()
            .map(|l| l.path.clone())
            .unwrap_or_default();
        if d.code.as_str() == "syntax" {
            prop_assert!(
                syntax_in.insert(path),
                "two syntax errors in one file: {}",
                d
            );
        } else {
            prop_assert!(reader_in.insert(path), "unexpected or repeated {}", d);
        }
    }
    prop_assert_eq!(syntax_in, junk);
    prop_assert_eq!(reader_in, reader);

    // The order of the files does not matter.
    prop_assert_eq!(load(f, true), diagnostics);
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    /// REQ-B-12: with clean files, loading terminates; `subgraph_cycle` appears once per cyclic
    /// strongly connected component at the position ASM-092 (c) gives; `subgraph_too_deep` appears
    /// at every reference of a graph at level == limit outside the cyclic components;
    /// `version_mismatch` appears exactly for the calls whose range the target's version misses;
    /// nothing else appears; and the files in another order give identical diagnostics.
    #[test]
    fn subgraph_cycles_depth_and_versions(f in forest(false)) {
        check(&f)?;
    }

    /// ASM-056, ASM-106 a: graphs with a syntax error, a reader error or a duplicate ID are
    /// known but unusable; calls to them cause no cascade (no `unknown_id`, no version check, no
    /// cycle or depth through them), and the rest of the forest is still checked as above.
    #[test]
    fn damaged_graphs_cause_no_cascade(f in forest(true)) {
        check(&f)?;
    }
}
