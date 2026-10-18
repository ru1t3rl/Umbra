//! The compiled, immutable form of a graph (ADR 0013): a node arena with resolved leaves, shared by
//! every entity that runs the graph. Per-entity state lives in [`crate::instance`].

use std::collections::BTreeMap;
use std::sync::Arc;

use cgf_core::{Action, BbType, Condition, GraphDef, NodeId, NodeKind, ParallelPolicy, Seconds};

use crate::factory::GraphRegistrationError;

/// The deepest node nesting the interpreter accepts. The tick recurses once per level, so this
/// bounds the stack use whatever a graph looks like (ASM-095).
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

fn children_of(kind: &NodeKind) -> Vec<NodeId> {
    match kind {
        NodeKind::Sequence(c) | NodeKind::Selector(c) => c.children.clone(),
        NodeKind::Parallel(p) => p.children.clone(),
        NodeKind::Inverter(d) | NodeKind::SucceedAlways(d) => vec![d.child],
        NodeKind::Repeat(r) => vec![r.child],
        NodeKind::Cooldown(t) | NodeKind::Timeout(t) => vec![t.child],
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
        if matches!(node.kind, NodeKind::Fsm(_) | NodeKind::Subgraph(_)) {
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
    Ok(())
}
