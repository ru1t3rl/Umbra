//! Registration of the built-in conditions and actions and of graphs as behaviors.
//!
//! `register_graphs` is WI-10; `register_builtins` (WI-11) delegates to the `leaves` module.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use cgf_core::{
    BehaviorId, ComponentId, ComponentInfo, ComponentKind, GraphDef, NodeKind, Registry,
    RegistryError,
};

use crate::factory::{GraphFactory, GraphRegistrationError};
use crate::leaves;

/// Registers the built-in conditions (`builtin.cond.{enemy_in_range, target_in_range,
/// timer_elapsed, chance}`) and actions (`builtin.action.{follow_route, move_to, wait, fire}`),
/// ASM-077.
///
/// # Errors
/// [`RegistryError::Duplicate`] if one of them is already registered.
pub fn register_builtins(registry: &mut Registry) -> Result<(), RegistryError> {
    leaves::register(registry)
}

/// The longest `subgraph` chain `register_graphs` accepts, counted in behaviors including the
/// first (ASM-073: the loader's default `max_subgraph_depth`).
const MAX_SUBGRAPH_DEPTH: usize = cgf_core::DEFAULT_MAX_SUBGRAPH_DEPTH as usize;

/// Adds graphs as behaviors, subgraphs first (dependency order, ties by ID), with
/// `ComponentInfo` from each graph (provenance, version, `param_types` from `Interface::types`,
/// outputs, subgraph references, result text).
///
/// The order depends on the IDs only, never on the input order (REQ-K-4). A subgraph target may
/// also be a behavior that is already in `registry` (a built-in or plugin behavior, or a graph
/// from an earlier call). The checks that need the whole batch (duplicate, dangling reference,
/// cycle, depth) run before anything is registered, and the batch is all-or-nothing: graphs are
/// staged in a clone of `registry` that replaces it only when every graph registered, so after
/// an error `registry` is unchanged (ASM-099 e).
///
/// # Errors
/// [`GraphRegistrationError`]: a duplicate ID, an unresolved reference, a cycle, a chain deeper
/// than 16, or a graph the interpreter cannot build.
pub fn register_graphs<'a>(
    registry: &mut Registry,
    graphs: impl IntoIterator<Item = &'a Arc<GraphDef>>,
) -> Result<(), GraphRegistrationError> {
    let mut batch = BTreeMap::new();
    for def in graphs {
        let id = component_id(def)?;
        let kind = match registry.kind_of(&id) {
            Some(kind) => Some(kind),
            None => batch
                .insert(id.clone(), Arc::clone(def))
                .map(|_| ComponentKind::Behavior),
        };
        if let Some(kind) = kind {
            return Err(RegistryError::Duplicate { kind, id }.into());
        }
    }
    let order = Order::new(registry, &batch).sorted()?;
    let mut staged = registry.clone();
    for id in order {
        let Some(def) = batch.get(&id) else {
            continue;
        };
        let factory = GraphFactory::new(Arc::clone(def), &staged)?;
        staged.register_behavior(Arc::new(factory), info_of(def))?;
    }
    *registry = staged;
    Ok(())
}

fn component_id(def: &GraphDef) -> Result<ComponentId, GraphRegistrationError> {
    ComponentId::new(def.id.as_str()).map_err(|e| GraphRegistrationError::Malformed {
        graph: def.id.clone(),
        reason: e.to_string(),
    })
}

/// The catalog metadata of a graph (ADR 0011, ADR 0014).
fn info_of(def: &GraphDef) -> ComponentInfo {
    let mut info = ComponentInfo::new(
        def.description.clone().unwrap_or_default(),
        def.source,
        def.version.clone(),
    );
    info.param_types.clone_from(&def.interface.types);
    for (port, out) in &def.interface.outputs {
        info = info.with_output(port.clone(), out.ty);
    }
    for node in &def.nodes {
        if let NodeKind::Subgraph(call) = &node.kind {
            info = info.with_subgraph(call.id.clone(), call.version.clone());
        }
    }
    match &def.interface.result {
        Some(text) => info.with_result(text.clone()),
        None => info,
    }
}

/// The dependency walk: the depth-first post-order over the batch in ID order, which puts every
/// subgraph before its callers. Errors name the nearest batch graph on the path (`blame`).
struct Order<'a> {
    registry: &'a Registry,
    batch: &'a BTreeMap<ComponentId, Arc<GraphDef>>,
    done: BTreeSet<ComponentId>,
    path: Vec<ComponentId>,
    out: Vec<ComponentId>,
    /// Longest chain (in behaviors) starting at each finished ID.
    depth: BTreeMap<ComponentId, usize>,
}

impl<'a> Order<'a> {
    fn new(registry: &'a Registry, batch: &'a BTreeMap<ComponentId, Arc<GraphDef>>) -> Self {
        Self {
            registry,
            batch,
            done: BTreeSet::new(),
            path: Vec::new(),
            out: Vec::new(),
            depth: BTreeMap::new(),
        }
    }

    fn sorted(mut self) -> Result<Vec<ComponentId>, GraphRegistrationError> {
        for (id, def) in self.batch {
            self.visit(id, &def.id)?;
        }
        Ok(self.out)
    }

    /// The subgraph targets of `id`, from the batch or from the registry's metadata; `None` if
    /// nothing by that ID exists.
    fn targets(&self, id: &ComponentId) -> Option<Vec<ComponentId>> {
        if let Some(def) = self.batch.get(id) {
            return Some(
                def.nodes
                    .iter()
                    .filter_map(|n| match &n.kind {
                        NodeKind::Subgraph(call) => Some(call.id.clone()),
                        _ => None,
                    })
                    .collect(),
            );
        }
        self.registry.behavior(id)?;
        let info = self.registry.info(id)?;
        Some(info.subgraphs.iter().map(|(id, _)| id.clone()).collect())
    }

    /// Visits `id` and returns the length of the longest chain from it. The chain from the walk's
    /// root through `id` must stay within [`MAX_SUBGRAPH_DEPTH`], also when `id` was finished
    /// earlier through another path.
    fn visit(
        &mut self,
        id: &ComponentId,
        blame: &BehaviorId,
    ) -> Result<usize, GraphRegistrationError> {
        let too_deep = || GraphRegistrationError::TooDeep {
            graph: blame.clone(),
        };
        if self.path.contains(id) {
            return Err(GraphRegistrationError::Cycle {
                graph: blame.clone(),
            });
        }
        if let Some(&d) = self.depth.get(id) {
            return if self.path.len() + d > MAX_SUBGRAPH_DEPTH {
                Err(too_deep())
            } else {
                Ok(d)
            };
        }
        if self.path.len() >= MAX_SUBGRAPH_DEPTH {
            return Err(too_deep());
        }
        let Some(targets) = self.targets(id) else {
            return Err(GraphRegistrationError::Unresolved {
                graph: blame.clone(),
                id: id.clone(),
            });
        };
        let own = self.batch.get(id).map(|def| def.id.clone());
        let blame = own.as_ref().unwrap_or(blame);
        self.path.push(id.clone());
        let mut deepest = 0;
        for target in &targets {
            deepest = deepest.max(self.visit(target, blame)?);
        }
        self.path.pop();
        if self.batch.contains_key(id) && self.done.insert(id.clone()) {
            self.out.push(id.clone());
        }
        self.depth.insert(id.clone(), deepest + 1);
        Ok(deepest + 1)
    }
}
