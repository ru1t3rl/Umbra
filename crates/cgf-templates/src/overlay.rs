//! The scoped loader for template resolution: the registry's schemas plus the interfaces of the
//! graphs of this load, so a template `[behavior] ref` to a graph goes through the same
//! `check_component` as a registered behavior (WI-14, ADR 0010, ASM-083, ASM-107).
//!
//! The graph-only checks (binding forms, binding-only params, values followed into the graph)
//! are added around that one path, never instead of it.

use cgf_core::{ComponentId, ComponentKind, ParamValue};

use crate::components::drop_root_missing;
use crate::diagnostic::Diagnostic;
use crate::graph_resolve::{Graphs, LoadedGraphs};
use crate::loader::{GraphScope, Loader};
use crate::schema::CompiledSchema;
use crate::value::{LTable, Origin};

/// A loader that knows the graphs of one load, and what it needs to check template uses of them.
pub(crate) struct Overlay<'a> {
    /// The registry loader plus a `Behavior` schema (the interface) for every usable graph, and
    /// the IDs and files that stay silent or get a note (ASM-054 rule 3, ASM-056).
    pub(crate) loader: Loader<'a>,
    graphs: Graphs<'a>,
}

impl<'a> Loader<'a> {
    /// The scoped loader for the graphs of `loaded`.
    pub(crate) fn overlay(&'a self, loaded: &'a LoadedGraphs) -> Overlay<'a> {
        let graphs = Graphs::new(self, loaded);
        let mut schemas = self.schemas.clone();
        let mut silent = graphs.silent();
        for graph in loaded.set.iter() {
            let Ok(id) = ComponentId::new(graph.id.as_str()) else {
                continue;
            };
            let compiled = CompiledSchema::compile(&graph.interface.params);
            // The reader compiled every interface fragment, so the assembled schema compiles.
            debug_assert!(
                compiled.is_ok(),
                "a usable graph's interface schema compiles"
            );
            match compiled {
                Ok(compiled) => {
                    schemas.insert((ComponentKind::Behavior, id), compiled);
                }
                // Stay silent rather than report an `unknown_id` for a graph that exists.
                Err(_) => {
                    silent.insert(id);
                }
            }
        }
        let scope = GraphScope {
            silent,
            unreadable: graphs.unreadable(),
        };
        Overlay {
            loader: Loader {
                registry: self.registry,
                schemas,
                options: self.options.clone(),
                scope,
            },
            graphs,
        }
    }
}

impl Overlay<'_> {
    /// Checks one component use of `kind`. Behaviors may be graphs of the load; every kind goes
    /// through `check_component`.
    pub(crate) fn check(
        &self,
        kind: ComponentKind,
        id: &ComponentId,
        id_at: &Origin,
        params: &LTable,
        out: &mut Vec<Diagnostic>,
    ) -> Option<ParamValue> {
        if kind != ComponentKind::Behavior {
            return self.loader.check_component(kind, id, id_at, params, out);
        }
        let mut found = Vec::new();
        let Some(clean) = self.graphs.check_use(id, params, false, &mut found) else {
            return self.loader.check_component(kind, id, id_at, params, out);
        };
        // The loader computes `missing_param` for a graph (binding-only params included).
        let mut schema = Vec::new();
        let json = self
            .loader
            .check_component(kind, id, id_at, &clean, &mut schema);
        let all = schema.clone();
        drop_root_missing(&mut schema, clean.origin());
        // Never drop a template without a diagnostic.
        if json.is_none() && found.is_empty() && schema.is_empty() {
            debug_assert!(false, "a failed behavior check reported nothing");
            schema = all;
        }
        let failed = !found.is_empty() || !schema.is_empty();
        out.extend(found);
        out.extend(schema);
        json.filter(|_| !failed)
    }

    /// Checks what a template or override writes itself: parameters key by key, without
    /// `missing_param` and without a trial build (ASM-056).
    pub(crate) fn check_own(
        &self,
        kind: ComponentKind,
        id: &ComponentId,
        params: Option<&LTable>,
        out: &mut Vec<Diagnostic>,
    ) {
        let Some(own) = params else { return };
        let clean = (kind == ComponentKind::Behavior)
            .then(|| self.graphs.check_use(id, own, true, out))
            .flatten();
        self.loader
            .check_own_params(kind, id, Some(clean.as_ref().unwrap_or(own)), out);
    }
}
