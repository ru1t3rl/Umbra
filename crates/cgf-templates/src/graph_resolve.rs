//! Behavior graph sets: the validated graphs of one load (ADR 0012, REQ-B-6, REQ-B-9, REQ-B-10,
//! REQ-B-11, REQ-B-12, REQ-B-14).
//!
//! Each file is read on its own (`graph_read`); the set checks follow: IDs are unique and not
//! registered, every graph is checked on its own (`own`: shape, interface, nodes), `subgraph`
//! references resolve and fit (`call`), and the reference graph has no cycle and is not too deep
//! (`cycles`). A graph that cannot be used (syntax error, duplicate ID) is known by its ID, so
//! graphs that call it get no cascade error, but they are still checked for what they say
//! themselves (ASM-056, ASM-106).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use cgf_core::{ComponentId, ComponentInfo, ComponentKind, GraphDef, NodeKind, Provenance};

use crate::components::article;
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::graph_read::{GraphRead, read_graph};
use crate::loader::Loader;
use crate::source::SourceFile;
use crate::value::Origin;
use cycles::Link;
use view::View;

mod bind;
mod call;
mod cycles;
mod leaf;
mod own;
mod view;

/// The behavior graphs of a load, by ID.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GraphSet {
    graphs: BTreeMap<ComponentId, Arc<GraphDef>>,
}

impl GraphSet {
    /// The graph with this ID.
    pub fn get(&self, id: &ComponentId) -> Option<&Arc<GraphDef>> {
        self.graphs.get(id)
    }

    /// All graphs in ID order.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<GraphDef>> {
        self.graphs.values()
    }

    /// How many graphs there are.
    pub fn len(&self) -> usize {
        self.graphs.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.graphs.is_empty()
    }
}

/// The graphs read from a set of files, and every diagnostic, unsorted.
pub(crate) struct LoadedGraphs {
    pub(crate) set: GraphSet,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

/// What an ID of a graph file stands for in the load.
enum Known {
    /// A clean graph with a unique ID: the index of its file.
    Usable(usize),
    /// The ID exists but cannot be used (syntax or other file errors, duplicate, or the ID of a
    /// registered component): references to it are not reported again (ASM-056).
    Unusable,
}

/// What a `subgraph` reference points at.
enum Resolved<'a> {
    /// A usable graph of this load.
    Graph(View<'a>),
    /// A registered behavior.
    Registered(&'a ComponentInfo),
    /// A graph that cannot be used; its error is reported elsewhere.
    Silent,
    /// Nothing of that ID.
    Unknown,
}

/// The files of a load and the facts about their IDs.
struct Set<'a> {
    loader: &'a Loader<'a>,
    reads: &'a [GraphRead],
    known: BTreeMap<ComponentId, Known>,
    /// Files with syntax errors whose `id` could not be recovered (ASM-054 rule 3).
    unreadable: Vec<PathBuf>,
}

impl<'a> Set<'a> {
    /// Finds the duplicate IDs (also against registered components, ASM-063) and records what
    /// each ID stands for.
    fn new(loader: &'a Loader<'a>, reads: &'a [GraphRead], out: &mut Vec<Diagnostic>) -> Self {
        let mut first: BTreeMap<ComponentId, (usize, &Origin)> = BTreeMap::new();
        let mut known = BTreeMap::new();
        let mut unreadable = Vec::new();
        for (index, read) in reads.iter().enumerate() {
            let Some((id, at)) = &read.known_id else {
                if read.syntax_errors {
                    unreadable.push(read.path.clone());
                }
                continue;
            };
            if let Some(kind) = loader.registry.kind_of(id) {
                let message = format!("graph `{id}` has the ID of a registered {}", kind.noun());
                let found =
                    Diagnostic::new(DiagnosticCode::DuplicateId, message, Some(at.location()))
                        .with_note(
                            format!(
                                "`{id}` is registered as {} {}",
                                article(kind.noun()),
                                kind.noun()
                            ),
                            None,
                        );
                out.push(found);
                known.insert(id.clone(), Known::Unusable);
            } else if let Some(&(_, origin)) = first.get(id) {
                let message = format!("graph `{id}` is defined more than once");
                out.push(
                    Diagnostic::new(DiagnosticCode::DuplicateId, message, Some(at.location()))
                        .with_note("first defined here", Some(origin.location())),
                );
                known.insert(id.clone(), Known::Unusable);
            } else {
                first.insert(id.clone(), (index, at));
            }
        }
        for (id, (index, _)) in first {
            let state = if reads[index].graph.is_some() {
                Known::Usable(index)
            } else {
                Known::Unusable
            };
            known.entry(id).or_insert(state);
        }
        Self {
            loader,
            reads,
            known,
            unreadable,
        }
    }

    fn resolve(&self, id: &ComponentId) -> Resolved<'_> {
        match self.known.get(id) {
            Some(Known::Usable(index)) => {
                View::of(&self.reads[*index]).map_or(Resolved::Silent, Resolved::Graph)
            }
            Some(Known::Unusable) => Resolved::Silent,
            None => match self.loader.registry.behavior(id) {
                Some(_) => self
                    .loader
                    .registry
                    .info(id)
                    .map_or(Resolved::Silent, Resolved::Registered),
                None => Resolved::Unknown,
            },
        }
    }

    /// The most graphs on a `subgraph` chain (`max_subgraph_depth`, at least 1).
    fn depth_limit(&self) -> usize {
        usize::try_from(self.loader.options.max_subgraph_depth)
            .unwrap_or(usize::MAX)
            .max(1)
    }

    /// `unknown_id` for a `subgraph` target, noting files that may define it (ASM-054 rule 3).
    fn unknown_behavior(&self, id: &ComponentId, at: &Origin) -> Diagnostic {
        let mut found = self.loader.unknown_id(ComponentKind::Behavior, id, at);
        for path in &self.unreadable {
            found = found.with_note(
                format!(
                    "may be defined in {}, which has syntax errors",
                    path.display()
                ),
                None,
            );
        }
        found
    }

    /// The references between the graphs that can take part in cycles: graphs with a unique ID
    /// and readable parts, sorted by ID, and for each its `subgraph` references in file order.
    fn links(&self) -> (Vec<ComponentId>, Vec<Vec<Link>>) {
        let vertices: BTreeMap<&ComponentId, View<'_>> = self
            .known
            .iter()
            .filter_map(|(id, state)| match state {
                Known::Usable(i) => View::of(&self.reads[*i]).map(|v| (id, v)),
                Known::Unusable => None,
            })
            .collect();
        let ids: Vec<ComponentId> = vertices.keys().map(|id| (*id).clone()).collect();
        let index: BTreeMap<&ComponentId, usize> = vertices
            .keys()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
        let links = vertices
            .values()
            .map(|view| {
                view.parts
                    .nodes
                    .iter()
                    .filter_map(|(name, def)| {
                        let NodeKind::Subgraph(call) = &def.as_ref()?.kind else {
                            return None;
                        };
                        let at = view.node_table(name)?.get("ref")?.origin().clone();
                        Some(Link {
                            to: *index.get(&call.id)?,
                            at,
                        })
                    })
                    .collect()
            })
            .collect();
        (ids, links)
    }
}

impl Loader<'_> {
    /// Reads graph files in path order, so the result does not depend on the order they are given
    /// in (REQ-K-4), then checks the set: IDs, every graph, references, cycles and depth (WI-13).
    /// Files with the same path keep their input order (the sort is stable), as in M2
    /// `load_templates`.
    pub(crate) fn load_graphs(&self, files: &[&SourceFile], source: Provenance) -> LoadedGraphs {
        let mut sorted: Vec<&SourceFile> = files.to_vec();
        sorted.sort_by(|a, b| a.path().cmp(b.path()));
        let reads: Vec<GraphRead> = sorted.iter().map(|f| read_graph(f, source)).collect();
        let mut diagnostics: Vec<Diagnostic> =
            reads.iter().flat_map(|r| r.diagnostics.clone()).collect();
        let set = Set::new(self, &reads, &mut diagnostics);
        let mut seen = call::Seen::new();
        for view in reads.iter().filter_map(View::of) {
            own::check(&set, &view, &mut seen, &mut diagnostics);
        }
        let (ids, links) = set.links();
        diagnostics.extend(cycles::analyse(&ids, &links, set.depth_limit()));
        let used: BTreeSet<&ComponentId> = set
            .known
            .iter()
            .filter(|(_, state)| matches!(state, Known::Usable(_)))
            .map(|(id, _)| id)
            .collect();
        let graphs = reads
            .iter()
            .filter_map(|r| r.graph.as_ref())
            .filter_map(|g| {
                let id = ComponentId::new(g.id.as_str()).ok()?;
                used.contains(&id).then(|| (id, Arc::new(g.clone())))
            })
            .collect();
        LoadedGraphs {
            set: GraphSet { graphs },
            diagnostics,
        }
    }
}
