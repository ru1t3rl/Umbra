//! The compiled, immutable form of a graph (ADR 0013): a node arena with resolved leaves, shared by
//! every entity that runs the graph. Per-entity state lives in [`crate::instance`].

use std::collections::BTreeMap;
use std::sync::Arc;

use cgf_core::{
    Action, BbType, Condition, FsmDef, GraphDef, NodeId, NodeKind, ParallelPolicy, Seconds, Status,
    TransitionTarget, Trigger,
};

use crate::factory::GraphRegistrationError;

/// The deepest node nesting one graph may have. The tick recurses once per level, so this bounds
/// the stack use of one graph; with subgraphs the bound multiplies by up to 16 graph levels
/// (`max_subgraph_depth`, ASM-095).
pub(crate) const MAX_NODE_DEPTH: usize = 128;

/// How a `parallel` node decides (ADR 0013).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Policy {
    All,
    Any,
}

/// One node with its children as arena indices.
pub(crate) enum Kind {
    Condition(Arc<dyn Condition>),
    Action(Arc<dyn Action>),
    Sequence {
        children: Box<[usize]>,
        memory: bool,
    },
    Selector {
        children: Box<[usize]>,
        memory: bool,
    },
    Parallel {
        children: Box<[usize]>,
        policy: Policy,
    },
    Inverter(usize),
    SucceedAlways(usize),
    Repeat {
        child: usize,
        count: Option<u32>,
    },
    Cooldown {
        child: usize,
        duration: Seconds,
    },
    Timeout {
        child: usize,
        duration: Seconds,
    },
    Fsm(Fsm),
}

/// Where a transition goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum To {
    /// A position in [`Fsm::states`].
    State(usize),
    /// A position in [`Fsm::exits`].
    Exit(usize),
}

/// What fires a transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum When {
    /// The guard node (arena index) returns Success.
    Guard(usize),
    /// The state node finished with this status.
    Done(Status),
}

/// One transition, in file order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Transition {
    pub from: usize,
    pub to: To,
    pub when: When,
}

/// A finite state machine (ADR 0013): state and guard nodes are arena indices.
pub(crate) struct Fsm {
    pub initial: usize,
    pub states: Box<[usize]>,
    pub exits: Box<[Status]>,
    pub transitions: Box<[Transition]>,
}

impl Kind {
    /// The node type name as written in graph files (for messages).
    pub(crate) fn type_name(&self) -> &'static str {
        match self {
            Kind::Condition(_) => "condition",
            Kind::Action(_) => "action",
            Kind::Sequence { .. } => "sequence",
            Kind::Selector { .. } => "selector",
            Kind::Parallel { .. } => "parallel",
            Kind::Inverter(_) => "inverter",
            Kind::SucceedAlways(_) => "succeed_always",
            Kind::Repeat { .. } => "repeat",
            Kind::Cooldown { .. } => "cooldown",
            Kind::Timeout { .. } => "timeout",
            Kind::Fsm(_) => "fsm",
        }
    }
}

/// A named node of the arena.
pub(crate) struct Node {
    pub name: Box<str>,
    pub kind: Kind,
}

/// A graph ready to be instantiated.
pub(crate) struct Program {
    pub nodes: Vec<Node>,
    pub root: usize,
    /// Declared blackboard keys.
    pub blackboard: BTreeMap<String, BbType>,
    /// Interface params, each a read-only key in the instance's blackboard (ADR 0014).
    pub inputs: BTreeMap<String, BbType>,
    /// Output port to blackboard key.
    pub outputs: BTreeMap<String, String>,
}

/// The arena index of `id`; `usize::MAX` (never a valid index) if it does not fit.
pub(crate) fn index(id: NodeId) -> usize {
    usize::try_from(id.0).unwrap_or(usize::MAX)
}

/// The policy of a `parallel` node, `None` for a policy this interpreter does not know.
pub(crate) fn policy(p: ParallelPolicy) -> Option<Policy> {
    match p {
        ParallelPolicy::All => Some(Policy::All),
        ParallelPolicy::Any => Some(Policy::Any),
        _ => None,
    }
}

/// The children of a node, in file order. `nodes/fsm.rs` `children` lists an FSM's children the
/// same way (states, then guards); keep them in step.
fn children_of(kind: &NodeKind) -> Vec<NodeId> {
    match kind {
        NodeKind::Sequence(c) | NodeKind::Selector(c) => c.children.clone(),
        NodeKind::Parallel(p) => p.children.clone(),
        NodeKind::Inverter(d) | NodeKind::SucceedAlways(d) => vec![d.child],
        NodeKind::Repeat(r) => vec![r.child],
        NodeKind::Cooldown(t) | NodeKind::Timeout(t) => vec![t.child],
        NodeKind::Fsm(f) => f
            .states
            .iter()
            .map(|s| s.node)
            .chain(f.transitions.iter().filter_map(|t| match t.trigger {
                Trigger::When(guard) => Some(guard),
                _ => None,
            }))
            .collect(),
        _ => Vec::new(),
    }
}

/// Checks that `def` is a tree of supported nodes within [`MAX_NODE_DEPTH`]: every index in range,
/// no node reached twice. Loaded graphs have passed the loader; code-built ones may not have.
pub(crate) fn validate(def: &GraphDef) -> Result<(), GraphRegistrationError> {
    let bad = |reason: String| GraphRegistrationError::Malformed {
        graph: def.id.clone(),
        reason,
    };
    for node in &def.nodes {
        let supported = match &node.kind {
            NodeKind::Condition(_)
            | NodeKind::Action(_)
            | NodeKind::Sequence(_)
            | NodeKind::Selector(_)
            | NodeKind::Inverter(_)
            | NodeKind::SucceedAlways(_)
            | NodeKind::Repeat(_)
            | NodeKind::Cooldown(_)
            | NodeKind::Timeout(_) => true,
            NodeKind::Fsm(f) => {
                fsm_shape(f).map_err(|reason| bad(format!("fsm `{}`: {reason}", node.name)))?;
                true
            }
            NodeKind::Parallel(p) => policy(p.policy).is_some(),
            _ => false,
        };
        if !supported {
            return Err(GraphRegistrationError::Unsupported {
                graph: def.id.clone(),
                node: node.name.clone(),
            });
        }
    }
    let root = index(def.root);
    if root >= def.nodes.len() {
        return Err(bad("the root node does not exist".to_string()));
    }
    let mut seen = vec![false; def.nodes.len()];
    let mut stack = vec![(root, 1usize)];
    while let Some((at, depth)) = stack.pop() {
        let Some(node) = def.nodes.get(at) else {
            return Err(bad("a child node does not exist".to_string()));
        };
        if std::mem::replace(&mut seen[at], true) {
            return Err(bad(format!("node `{}` is reached twice", node.name)));
        }
        if depth > MAX_NODE_DEPTH {
            return Err(bad(format!(
                "nodes are nested deeper than {MAX_NODE_DEPTH} levels at `{}`",
                node.name
            )));
        }
        stack.extend(
            children_of(&node.kind)
                .into_iter()
                .map(|c| (index(c), depth + 1)),
        );
    }
    // The arena is a tree now, so the guard walks below terminate.
    for (node, _) in def.nodes.iter().zip(&seen).filter(|(_, s)| **s) {
        if let NodeKind::Fsm(f) = &node.kind {
            for t in &f.transitions {
                if let Trigger::When(guard) = t.trigger {
                    guard_shape(def, guard)
                        .map_err(|reason| bad(format!("fsm `{}`: {reason}", node.name)))?;
                }
            }
        }
    }
    Ok(())
}

/// Checks an FSM: `initial`, `from` and targets in range, exits and `on` triggers only Success or
/// Failure (so an FSM is Running exactly when it has not exited). Guard subtrees are checked by
/// [`guard_shape`] once the tree walk has proved the arena is a tree.
fn fsm_shape(f: &FsmDef) -> Result<(), String> {
    if f.initial >= f.states.len() {
        return Err("the initial state does not exist".to_string());
    }
    if f.exits.iter().any(|e| e.result == Status::Running) {
        return Err("an exit maps to `running`; only success and failure are allowed".to_string());
    }
    for t in &f.transitions {
        let target_ok = match t.to {
            TransitionTarget::State(s) => s < f.states.len(),
            TransitionTarget::Exit(e) => e < f.exits.len(),
        };
        if t.from >= f.states.len() || !target_ok {
            return Err("a transition names a state or exit that does not exist".to_string());
        }
        match t.trigger {
            Trigger::On(Status::Running) => {
                return Err("an `on running` transition is not allowed".to_string());
            }
            Trigger::When(_) | Trigger::On(_) => {}
            _ => {
                return Err("a transition has a trigger this interpreter does not know".to_string());
            }
        }
    }
    Ok(())
}

/// Walks a guard subtree and refuses any node that could return Running (ADR 0012: only
/// `condition`, `inverter`, `sequence` and `selector`, so a guard never needs a halt). Only call
/// it on a node of a proved tree: it keeps no visited set.
fn guard_shape(def: &GraphDef, guard: NodeId) -> Result<(), String> {
    let mut stack = vec![index(guard)];
    while let Some(at) = stack.pop() {
        let Some(node) = def.nodes.get(at) else {
            return Ok(());
        };
        match &node.kind {
            NodeKind::Condition(_)
            | NodeKind::Inverter(_)
            | NodeKind::Sequence(_)
            | NodeKind::Selector(_) => {}
            other => {
                return Err(format!(
                    "guard `{}` contains a `{}` node; guards may contain only condition, \
                     inverter, sequence and selector",
                    node.name,
                    kind_name(other)
                ));
            }
        }
        stack.extend(children_of(&node.kind).into_iter().map(index));
    }
    Ok(())
}

fn kind_name(kind: &NodeKind) -> &'static str {
    kind.tag().name()
}

/// The FSM of `def`, already checked by [`fsm_shape`] through [`validate`].
pub(crate) fn fsm(def: &FsmDef) -> Option<Fsm> {
    let transitions = def
        .transitions
        .iter()
        .map(|t| {
            let to = match t.to {
                TransitionTarget::State(s) => To::State(s),
                TransitionTarget::Exit(e) => To::Exit(e),
            };
            let when = match t.trigger {
                Trigger::When(g) => When::Guard(index(g)),
                Trigger::On(s) => When::Done(s),
                _ => return None,
            };
            Some(Transition {
                from: t.from,
                to,
                when,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Fsm {
        initial: def.initial,
        states: def.states.iter().map(|s| index(s.node)).collect(),
        exits: def.exits.iter().map(|e| e.result).collect(),
        transitions: transitions.into_boxed_slice(),
    })
}
