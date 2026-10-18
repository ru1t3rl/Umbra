//! Cycles and depth of the `subgraph` reference graph (REQ-B-12, ASM-073, ASM-092 c).
//!
//! Pure over indices: vertex `i` is the graph with the `i`-th smallest ID, so results never depend
//! on the order the files were given in (REQ-K-4). A cycle is reported once per strongly connected
//! component; depth is measured on the graphs outside every cycle.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, VecDeque};

use umbra_core::ComponentId;

use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::value::Origin;

/// A `subgraph` node's reference to another graph of the load.
pub(super) struct Link {
    /// Index of the target graph.
    pub(super) to: usize,
    /// The `ref` value of the node.
    pub(super) at: Origin,
}

/// A `subgraph` node's reference to a registered behavior: the chain it starts counts one level
/// per behavior (`Registry::longest_subgraph_chain`, ASM-099 e), as `register_graphs` counts it.
pub(super) struct Tail {
    /// The longest chain from the registered target down, target first.
    pub(super) chain: Vec<ComponentId>,
    /// The `ref` value of the node.
    pub(super) at: Origin,
}

/// Every `subgraph_cycle` and `subgraph_too_deep` of the reference graph. `ids` are sorted;
/// `links[v]` are the references of graph `v` in file order and `tails[v]` its references to
/// registered behaviors; `limit` is the most behaviors on one chain.
pub(super) fn analyse(
    ids: &[ComponentId],
    links: &[Vec<Link>],
    tails: &[Vec<Tail>],
    limit: usize,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let mut cyclic = vec![false; ids.len()];
    for members in components(links) {
        let looped = members.len() > 1 || links[members[0]].iter().any(|l| l.to == members[0]);
        if looped {
            out.push(cycle(ids, links, &members));
            for &m in &members {
                cyclic[m] = true;
            }
        }
    }
    out.extend(too_deep(ids, links, tails, &cyclic, limit));
    out
}

/// Strongly connected components (iterative Tarjan); each lists its members in ascending order.
fn components(links: &[Vec<Link>]) -> Vec<Vec<usize>> {
    let n = links.len();
    let mut index: Vec<Option<usize>> = vec![None; n];
    let mut low = vec![0usize; n];
    let mut on_stack = vec![false; n];
    let mut stack = Vec::new();
    let mut found = Vec::new();
    let mut next = 0;
    for start in 0..n {
        if index[start].is_some() {
            continue;
        }
        let mut frames = vec![(start, 0usize)];
        index[start] = Some(next);
        low[start] = next;
        next += 1;
        stack.push(start);
        on_stack[start] = true;
        while let Some(&(v, i)) = frames.last() {
            if let Some(link) = links[v].get(i) {
                if let Some(top) = frames.last_mut() {
                    top.1 += 1;
                }
                let w = link.to;
                match index[w] {
                    None => {
                        index[w] = Some(next);
                        low[w] = next;
                        next += 1;
                        stack.push(w);
                        on_stack[w] = true;
                        frames.push((w, 0));
                    }
                    Some(iw) if on_stack[w] => low[v] = low[v].min(iw),
                    Some(_) => {}
                }
                continue;
            }
            frames.pop();
            if Some(low[v]) == index[v] {
                let mut members = Vec::new();
                while let Some(w) = stack.pop() {
                    on_stack[w] = false;
                    members.push(w);
                    if w == v {
                        break;
                    }
                }
                members.sort_unstable();
                found.push(members);
            }
            if let Some(&(parent, _)) = frames.last() {
                low[parent] = low[parent].min(low[v]);
            }
        }
    }
    found
}

/// The first reference of each target of `v` inside `members`, by target.
fn successors<'l>(
    links: &'l [Vec<Link>],
    v: usize,
    members: &[usize],
) -> BTreeMap<usize, &'l Link> {
    let mut found = BTreeMap::new();
    for link in &links[v] {
        if members.binary_search(&link.to).is_ok() {
            found.entry(link.to).or_insert(link);
        }
    }
    found
}

/// The diagnostic of one cyclic component: at the first reference of the shortest cycle through
/// its smallest graph (ties by target ID), with a note at each further reference.
fn cycle(ids: &[ComponentId], links: &[Vec<Link>], members: &[usize]) -> Diagnostic {
    let start = members[0];
    let mut parent: BTreeMap<usize, usize> = BTreeMap::new();
    let mut queue = VecDeque::from([start]);
    let mut closing = start;
    'search: while let Some(v) = queue.pop_front() {
        for &w in successors(links, v, members).keys() {
            if w == start {
                closing = v;
                break 'search;
            }
            if let Entry::Vacant(slot) = parent.entry(w) {
                slot.insert(v);
                queue.push_back(w);
            }
        }
    }
    let mut path = vec![closing];
    while let Some(&p) = parent.get(&path[path.len() - 1]) {
        path.push(p);
    }
    path.reverse();
    path.push(start);
    let names: Vec<String> = path.iter().map(|&v| format!("`{}`", ids[v])).collect();
    let message = format!("subgraph cycle: {}", names.join(" -> "));
    let mut refs = path
        .windows(2)
        .filter_map(|w| successors(links, w[0], members).remove(&w[1]));
    let first = refs.next();
    let mut diagnostic = Diagnostic::new(
        DiagnosticCode::SubgraphCycle,
        message,
        first.map(|l| l.at.location()),
    );
    for (link, w) in refs.zip(path.windows(2).skip(1)) {
        let note = format!("`{}` calls `{}` here", ids[w[0]], ids[w[1]]);
        diagnostic = diagnostic.with_note(note, Some(link.at.location()));
    }
    diagnostic
}

/// `subgraph_too_deep` at each reference that would make a chain of acyclic graphs longer than
/// `limit` graphs: the references from a graph whose longest chain from a root is exactly `limit`,
/// and the references to a registered chain that does not fit below the graph (the note names the
/// chain). Graphs on cycles are left out (their cycle is reported).
fn too_deep(
    ids: &[ComponentId],
    links: &[Vec<Link>],
    tails: &[Vec<Tail>],
    cyclic: &[bool],
    limit: usize,
) -> Vec<Diagnostic> {
    let n = links.len();
    let edges = |v: usize| links[v].iter().filter(|l| !cyclic[l.to]);
    let mut waiting = vec![0usize; n];
    for v in (0..n).filter(|&v| !cyclic[v]) {
        for link in edges(v) {
            waiting[link.to] += 1;
        }
    }
    let mut level = vec![1usize; n];
    let mut ready: Vec<usize> = (0..n).filter(|&v| !cyclic[v] && waiting[v] == 0).collect();
    while let Some(v) = ready.pop() {
        for link in edges(v) {
            level[link.to] = level[link.to].max(level[v] + 1);
            waiting[link.to] -= 1;
            if waiting[link.to] == 0 {
                ready.push(link.to);
            }
        }
    }
    let mut out = Vec::new();
    for v in (0..n).filter(|&v| !cyclic[v] && level[v] == limit) {
        for link in edges(v) {
            let message = format!(
                "`{}` calls `{}`, which makes the chain deeper than the limit of {limit} behaviors",
                ids[v], ids[link.to]
            );
            out.push(Diagnostic::new(
                DiagnosticCode::SubgraphTooDeep,
                message,
                Some(link.at.location()),
            ));
        }
    }
    for v in (0..n).filter(|&v| !cyclic[v]) {
        for tail in tails[v]
            .iter()
            .filter(|t| level[v] <= limit && level[v] + t.chain.len() > limit)
        {
            out.push(registered_too_deep(&ids[v], tail, limit));
        }
    }
    out
}

/// `subgraph_too_deep` at a reference to a registered chain that crosses the limit.
fn registered_too_deep(caller: &ComponentId, tail: &Tail, limit: usize) -> Diagnostic {
    let names: Vec<String> = tail.chain.iter().map(|id| format!("`{id}`")).collect();
    let message = format!(
        "`{caller}` calls `{}`, which makes the chain deeper than the limit of {limit} behaviors",
        tail.chain[0]
    );
    let note = format!(
        "registered behaviors count one level each; the registered chain is {}",
        names.join(" -> ")
    );
    Diagnostic::new(
        DiagnosticCode::SubgraphTooDeep,
        message,
        Some(tail.at.location()),
    )
    .with_note(note, None)
}
