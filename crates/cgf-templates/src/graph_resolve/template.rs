//! The graphs of a load as seen by template `[behavior]` sections (WI-14, ADR 0014, ASM-083,
//! ASM-107): which IDs are graphs, which are known but unusable, and the call-site checks of a
//! template's `params`, shared with `subgraph` nodes.

use std::collections::BTreeSet;
use std::path::PathBuf;

use cgf_core::ComponentId;

use super::{Known, LoadedGraphs, Resolved, Set, call};
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::loader::Loader;
use crate::value::{LEntry, LTable};

use super::bind::forms;

/// The graph files of a load, ready to check template uses against.
pub(crate) struct Graphs<'a> {
    set: Set<'a>,
}

impl<'a> Graphs<'a> {
    /// The facts about the IDs of `loaded`. `loader` is the plain loader (its registry decides
    /// which IDs clash); the set-level diagnostics were reported by `load_graphs` already.
    pub(crate) fn new(loader: &'a Loader<'a>, graphs: &'a LoadedGraphs) -> Self {
        let mut ignored = Vec::new();
        Self {
            set: Set::new(loader, &graphs.reads, &mut ignored),
        }
    }

    /// IDs that exist but cannot be used (ASM-056): naming one is silent.
    pub(crate) fn silent(&self) -> BTreeSet<ComponentId> {
        self.set
            .known
            .iter()
            .filter(|(_, state)| matches!(state, Known::Unusable))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Graph files with syntax errors whose ID is gone.
    pub(crate) fn unreadable(&self) -> Vec<PathBuf> {
        self.set.unreadable.clone()
    }

    /// Checks a template's use of `id` as a top-level behavior when `id` is a usable graph, and
    /// returns the params the schema check should see: `params` without the entries that hold a
    /// `{ key }` or `{ param }` form (reported `invalid_binding` here: a top-level behavior has no
    /// blackboard and no enclosing interface). `None` when `id` is not a usable graph.
    ///
    /// Required params that are missing (binding-only ones included) are reported unless
    /// `partial`, and every supplied value is followed down the chain like a call-site value.
    pub(crate) fn check_use(
        &self,
        id: &ComponentId,
        params: &LTable,
        partial: bool,
        out: &mut Vec<Diagnostic>,
    ) -> Option<LTable> {
        if !matches!(self.set.resolve(id), Resolved::Graph(_)) {
            return None;
        }
        let mut bound = BTreeSet::new();
        for form in forms(params) {
            let message = if form.is_key {
                "`{ key = .. }` is not allowed in a template's `[behavior]` params: a top-level behavior has no blackboard to bind from"
            } else {
                "`{ param = .. }` is not allowed in a template's `[behavior]` params: it has no enclosing graph interface"
            };
            out.push(Diagnostic::new(
                DiagnosticCode::InvalidBinding,
                message,
                Some(form.value.origin().location()),
            ));
            bound.insert(form.top.to_string());
        }
        let entries = params
            .iter()
            .filter(|(key, _)| !bound.contains(*key))
            .map(|(key, entry): (&str, &LEntry)| (key.to_string(), entry.clone()))
            .collect();
        let clean = LTable::from_entries(params.origin().clone(), entries);
        call::template_use(&self.set, id, (&clean, &bound), partial, out);
        Some(clean)
    }
}
