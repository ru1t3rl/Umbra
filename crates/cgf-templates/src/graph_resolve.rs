//! Behavior graph sets: the validated graphs of one load (ADR 0012, REQ-B-6, REQ-B-12).
//!
//! WI-12 reads each graph file on its own (`graph_read`) and collects the graphs; the set-level
//! resolution (duplicates, references, cycles, versions, bindings, call sites) is WI-13.

use std::collections::BTreeMap;
use std::sync::Arc;

use cgf_core::{ComponentId, GraphDef, Provenance};

use crate::diagnostic::Diagnostic;
use crate::graph_read::read_graph;
use crate::loader::Loader;
use crate::source::SourceFile;

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

impl Loader<'_> {
    /// Reads graph files in path order, so the result does not depend on the order they are given
    /// in (REQ-K-4). Per-file checks only (WI-12): when two files define one ID, the first path
    /// wins here and WI-13 reports the duplicate (ASM-101). Files with the same path keep their input
    /// order (the sort is stable), as in M2 `load_templates`.
    #[expect(
        clippy::unused_self,
        reason = "WI-13 checks the graphs against the registry"
    )]
    pub(crate) fn load_graphs(&self, files: &[&SourceFile], source: Provenance) -> LoadedGraphs {
        let mut sorted: Vec<&SourceFile> = files.to_vec();
        sorted.sort_by(|a, b| a.path().cmp(b.path()));
        let mut graphs = BTreeMap::new();
        let mut diagnostics = Vec::new();
        for file in sorted {
            let read = read_graph(file, source);
            diagnostics.extend(read.diagnostics);
            // `BehaviorId` and `ComponentId` share one grammar, so the conversion cannot fail.
            if let Some(graph) = read.graph
                && let Ok(id) = ComponentId::new(graph.id.as_str())
            {
                graphs.entry(id).or_insert_with(|| Arc::new(graph));
            }
        }
        LoadedGraphs {
            set: GraphSet { graphs },
            diagnostics,
        }
    }
}
