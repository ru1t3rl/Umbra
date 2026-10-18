//! Registration of the built-in conditions and actions and of graphs as behaviors.
//!
//! `register_graphs` is WI-10; `register_builtins` is a signature stub until WI-11.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use cgf_core::{
    BehaviorId, ComponentId, ComponentInfo, ComponentKind, GraphDef, NodeKind, Registry,
    RegistryError,
};

use crate::factory::{GraphFactory, GraphRegistrationError};

/// Registers the built-in conditions (`builtin.cond.{enemy_in_range, target_in_range,
/// timer_elapsed, chance}`) and actions (`builtin.action.{follow_route, move_to, wait, fire}`),
/// ASM-077.
///
/// # Errors
/// [`RegistryError::Duplicate`] if one of them is already registered.
#[expect(clippy::todo, reason = "M3 WI-11 stub")]
pub fn register_builtins(_registry: &mut Registry) -> Result<(), RegistryError> {
    todo!("M3 WI-11")
}

/// The longest `subgraph` chain `register_graphs` accepts, counted in behaviors including the
/// first (ASM-073: the loader's default `max_subgraph_depth`).
const MAX_SUBGRAPH_DEPTH: usize = 16;

/// Adds graphs as behaviors, subgraphs first (dependency order, ties by ID), with
/// `ComponentInfo` from each graph (provenance, version, `param_types` from `Interface::types`,
/// outputs, subgraph references, result text).
///
/// The order depends on the IDs only, never on the input order (REQ-K-4). A subgraph target may
/// also be a behavior that is already in `registry` (a built-in or plugin behavior, or a graph
/// from an earlier call). The checks that need the whole batch (duplicate, dangling reference,
/// cycle, depth) run before anything is registered; a failure while building or registering
/// a graph leaves the earlier graphs of the batch in the registry (ASM-091 d: callers register
/// into a clone).
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
    let Some(first) = batch.values().next() else {
        return Ok(());
    };
    let order = Order::new(registry, &batch, first.id.clone()).sorted()?;
    for id in order {
        let Some(def) = batch.get(&id) else {
            continue;
        };
        let factory = GraphFactory::new(Arc::clone(def), registry)?;
        registry.register_behavior(Arc::new(factory), info_of(def))?;
    }
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
/// subgraph before its callers.
struct Order<'a> {
    registry: &'a Registry,
    batch: &'a BTreeMap<ComponentId, Arc<GraphDef>>,
    done: BTreeSet<ComponentId>,
    path: Vec<ComponentId>,
    out: Vec<ComponentId>,
    /// Longest chain (in behaviors) starting at each visited ID.
    depth: BTreeMap<ComponentId, usize>,
    /// The batch graph being walked from.
    root: BehaviorId,
}

impl<'a> Order<'a> {
    fn new(
        registry: &'a Registry,
        batch: &'a BTreeMap<ComponentId, Arc<GraphDef>>,
        root: BehaviorId,
    ) -> Self {
        Self {
            registry,
            batch,
            done: BTreeSet::new(),
            path: Vec::new(),
            out: Vec::new(),
            depth: BTreeMap::new(),
            root,
        }
    }

    fn sorted(mut self) -> Result<Vec<ComponentId>, GraphRegistrationError> {
        for (id, def) in self.batch {
            self.root = def.id.clone();
            self.visit(id)?;
        }
        Ok(self.out)
    }

    /// `id` as a behavior ID (the grammars are the same, so the fallback is never used).
    fn behavior_id(&self, id: &ComponentId) -> BehaviorId {
        BehaviorId::new(id.as_str()).unwrap_or_else(|_| self.root.clone())
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

    fn visit(&mut self, id: &ComponentId) -> Result<usize, GraphRegistrationError> {
        if let Some(&d) = self.depth.get(id) {
            return Ok(d);
        }
        if self.path.contains(id) {
            return Err(GraphRegistrationError::Cycle {
                graph: self.behavior_id(id),
            });
        }
        if self.path.len() >= MAX_SUBGRAPH_DEPTH {
            return Err(GraphRegistrationError::TooDeep {
                graph: self.behavior_id(id),
            });
        }
        let Some(targets) = self.targets(id) else {
            // The caller is the last path entry (a top-level miss cannot happen: it is in the batch).
            let caller = self.path.last().unwrap_or(id);
            return Err(GraphRegistrationError::Unresolved {
                graph: self.behavior_id(caller),
                id: id.clone(),
            });
        };
        self.path.push(id.clone());
        let mut deepest = 0;
        for target in &targets {
            deepest = deepest.max(self.visit(target)?);
        }
        self.path.pop();
        if self.batch.contains_key(id) && self.done.insert(id.clone()) {
            self.out.push(id.clone());
        }
        self.depth.insert(id.clone(), deepest + 1);
        Ok(deepest + 1)
    }
}
