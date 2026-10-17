//! Behavior graph sets: the validated graphs of one load (ADR 0012, REQ-B-6, REQ-B-12).
//!
//! M3 step 4: only the result type exists; the resolution (WI-13) and the file reader (WI-12)
//! arrive with their work items.

use std::collections::BTreeMap;
use std::sync::Arc;

use cgf_core::{ComponentId, GraphDef};

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
