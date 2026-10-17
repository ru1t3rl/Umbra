//! The validated, format-free behavior graph (REQ-B-6, REQ-B-7, REQ-B-10, REQ-B-14, ADR 0011,
//! ADR 0012, ADR 0014, ADR 0017).
//!
//! `cgf-templates` produces a [`GraphDef`] from a `*.behavior.toml` file (or code builds one);
//! `cgf-behavior` compiles it. The model carries no editor data (REQ-B-16) and uses only ordered
//! collections, so every iteration order is deterministic: nodes and FSM tables are `Vec`s in file
//! order, maps are `BTreeMap`s.

use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::path::PathBuf;

pub use semver::{Version, VersionReq};

use crate::behavior::{BbType, Status};
use crate::catalog::Provenance;
use crate::id::{BehaviorId, ComponentId};
use crate::params::{ParamSchema, ParamValue};
use crate::units::Seconds;

/// Index of a node in [`GraphDef::nodes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeId(pub u32);

/// A behavior graph: tree of nodes plus interface and blackboard declarations.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphDef {
    /// Globally unique graph ID (ASM-063).
    pub id: BehaviorId,
    /// Semantic version (REQ-B-14).
    pub version: Version,
    /// Optional description.
    pub description: Option<String>,
    /// Where the graph came from (REQ-B-13).
    pub source: Provenance,
    /// File path for messages; `None` for code-built graphs.
    pub origin: Option<PathBuf>,
    /// Parameters, outputs and result (ADR 0014).
    pub interface: Interface,
    /// Declared blackboard keys (not the interface params, which are implicit read-only keys).
    pub blackboard: BTreeMap<String, BbType>,
    /// The root node.
    pub root: NodeId,
    /// All nodes in file order; [`NodeId`] indexes this vector.
    pub nodes: Vec<NodeDef>,
}

impl GraphDef {
    /// The node with `id`, or `None` when out of range.
    pub fn node(&self, id: NodeId) -> Option<&NodeDef> {
        self.nodes.get(usize::try_from(id.0).ok()?)
    }

    /// Referenced subgraphs with their version ranges, in node order.
    pub fn subgraph_refs(&self) -> Vec<(&ComponentId, Option<&VersionReq>)> {
        self.nodes
            .iter()
            .filter_map(|n| match &n.kind {
                NodeKind::Subgraph(s) => Some((&s.id, s.version.as_ref())),
                _ => None,
            })
            .collect()
    }

    /// Referenced conditions and actions with their tag, in node order.
    pub fn leaf_refs(&self) -> Vec<(NodeKindTag, &ComponentId)> {
        self.nodes
            .iter()
            .filter_map(|n| match &n.kind {
                NodeKind::Condition(l) => Some((NodeKindTag::Condition, &l.id)),
                NodeKind::Action(l) => Some((NodeKindTag::Action, &l.id)),
                _ => None,
            })
            .collect()
    }
}

/// A graph's public surface (ADR 0014).
#[derive(Debug, Clone, PartialEq)]
pub struct Interface {
    /// Object schema assembled from the literal-capable params.
    pub params: ParamSchema,
    /// Every param's blackboard type, including binding-only `entity` and `position` params.
    pub types: BTreeMap<String, BbType>,
    /// Default values of params that have one.
    pub defaults: BTreeMap<String, ParamValue>,
    /// Output ports by name.
    pub outputs: BTreeMap<String, OutputPort>,
    /// The output port that carries the result, if any.
    pub result: Option<String>,
}

impl Interface {
    /// Whether `param` is declared and has no default.
    pub fn is_required(&self, param: &str) -> bool {
        self.types.contains_key(param) && !self.defaults.contains_key(param)
    }

    /// Whether `param` can only be bound at run time (`entity` or `position`).
    pub fn is_binding_only(&self, param: &str) -> bool {
        matches!(
            self.types.get(param),
            Some(BbType::Entity | BbType::Position)
        )
    }
}

/// An output port: its type and the blackboard key it reads at the end of a tick.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputPort {
    /// Port type.
    pub ty: BbType,
    /// Blackboard key the port is copied from.
    pub from: String,
}

/// A named node.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeDef {
    /// Node name (`[a-z][a-z0-9_]*`), unique in the graph.
    pub name: String,
    /// Type and settings.
    pub kind: NodeKind,
}

/// Node type and its settings (REQ-B-7). Children are [`NodeId`]s in array order.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum NodeKind {
    /// Runs children in order until one fails or is Running.
    Sequence {
        /// Children in order.
        children: Vec<NodeId>,
        /// Resume at the Running child instead of re-checking from the first.
        memory: bool,
    },
    /// Runs children in order until one succeeds or is Running.
    Selector {
        /// Children in order.
        children: Vec<NodeId>,
        /// Resume at the Running child instead of re-checking from the first.
        memory: bool,
    },
    /// Ticks every unfinished child each tick.
    Parallel {
        /// Children in order.
        children: Vec<NodeId>,
        /// When the node finishes.
        policy: ParallelPolicy,
    },
    /// Swaps Success and Failure of the child.
    Inverter {
        /// The child.
        child: NodeId,
    },
    /// Maps the child's Failure to Success.
    SucceedAlways {
        /// The child.
        child: NodeId,
    },
    /// Repeats the child.
    Repeat {
        /// The child.
        child: NodeId,
        /// Successes needed; `None` repeats forever.
        count: Option<NonZeroU32>,
    },
    /// Blocks the child for `duration` after it finishes.
    Cooldown {
        /// The child.
        child: NodeId,
        /// Cooldown length (> 0).
        duration: Seconds,
    },
    /// Aborts the child when it runs longer than `duration`.
    Timeout {
        /// The child.
        child: NodeId,
        /// Time limit (> 0).
        duration: Seconds,
    },
    /// A condition leaf.
    Condition(LeafRef),
    /// An action leaf.
    Action(LeafRef),
    /// Another graph run with its own blackboard.
    Subgraph(SubgraphRef),
    /// A finite state machine.
    Fsm(FsmDef),
}

impl NodeKind {
    /// The node type tag.
    pub fn tag(&self) -> NodeKindTag {
        match self {
            Self::Sequence { .. } => NodeKindTag::Sequence,
            Self::Selector { .. } => NodeKindTag::Selector,
            Self::Parallel { .. } => NodeKindTag::Parallel,
            Self::Inverter { .. } => NodeKindTag::Inverter,
            Self::SucceedAlways { .. } => NodeKindTag::SucceedAlways,
            Self::Repeat { .. } => NodeKindTag::Repeat,
            Self::Cooldown { .. } => NodeKindTag::Cooldown,
            Self::Timeout { .. } => NodeKindTag::Timeout,
            Self::Condition(_) => NodeKindTag::Condition,
            Self::Action(_) => NodeKindTag::Action,
            Self::Subgraph(_) => NodeKindTag::Subgraph,
            Self::Fsm(_) => NodeKindTag::Fsm,
        }
    }
}

/// The node type of a [`NodeKind`] without its settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum NodeKindTag {
    /// `sequence`
    Sequence,
    /// `selector`
    Selector,
    /// `parallel`
    Parallel,
    /// `inverter`
    Inverter,
    /// `succeed_always`
    SucceedAlways,
    /// `repeat`
    Repeat,
    /// `cooldown`
    Cooldown,
    /// `timeout`
    Timeout,
    /// `condition`
    Condition,
    /// `action`
    Action,
    /// `subgraph`
    Subgraph,
    /// `fsm`
    Fsm,
}

impl NodeKindTag {
    /// The node type name used in graph files and in [`NODE_TYPES`].
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sequence => "sequence",
            Self::Selector => "selector",
            Self::Parallel => "parallel",
            Self::Inverter => "inverter",
            Self::SucceedAlways => "succeed_always",
            Self::Repeat => "repeat",
            Self::Cooldown => "cooldown",
            Self::Timeout => "timeout",
            Self::Condition => "condition",
            Self::Action => "action",
            Self::Subgraph => "subgraph",
            Self::Fsm => "fsm",
        }
    }
}

/// How a `parallel` node finishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParallelPolicy {
    /// Success when all succeeded, Failure on the first failure.
    All,
    /// Success on the first success, Failure when all failed.
    Any,
}

/// A condition or action leaf: catalog ID and parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct LeafRef {
    /// Catalog ID.
    pub id: ComponentId,
    /// Parameters, possibly with `{ param }` references.
    pub params: ParamTemplate,
}

/// A `subgraph` node.
#[derive(Debug, Clone, PartialEq)]
pub struct SubgraphRef {
    /// The referenced graph.
    pub id: ComponentId,
    /// Accepted versions; `None` accepts any.
    pub version: Option<VersionReq>,
    /// Literals and `{ param }` references (build time).
    pub params: ParamTemplate,
    /// Child param to parent key, from `{ key = .. }` values (run time).
    pub inputs: BTreeMap<String, String>,
    /// Child output port to parent key.
    pub outputs: BTreeMap<String, String>,
}

/// Parameters that may hold `{ param = "<name>" }` build-time references (ASM-076).
///
/// A reference is a JSON object with exactly one key, `param`, whose value is a string; it
/// replaces a whole value. Objects with more keys, or a non-string value, are literals. `{ key = .. }`
/// bindings never appear here: the reader moves them to [`SubgraphRef::inputs`].
#[derive(Debug, Clone, PartialEq)]
pub struct ParamTemplate(ParamValue);

/// A `{ param }` reference with no value in the arguments.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("parameter reference `{name}` at `{pointer}` has no value")]
pub struct UnboundParam {
    /// JSON pointer of the reference inside the template.
    pub pointer: String,
    /// The referenced interface parameter.
    pub name: String,
}

/// The name of a reference table, or `None` for a literal.
fn reference_name(v: &ParamValue) -> Option<&str> {
    let map = v.as_object()?;
    if map.len() != 1 {
        return None;
    }
    map.get("param")?.as_str()
}

fn escape_token(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

impl ParamTemplate {
    /// Wraps a parameter value.
    pub fn new(v: ParamValue) -> Self {
        Self(v)
    }

    /// The raw value, references included.
    pub fn as_json(&self) -> &ParamValue {
        &self.0
    }

    /// Every reference as (JSON pointer, parameter name). Order: depth first, array order, object
    /// keys sorted.
    pub fn references(&self) -> Vec<(String, &str)> {
        fn walk<'a>(v: &'a ParamValue, ptr: &mut String, out: &mut Vec<(String, &'a str)>) {
            if let Some(name) = reference_name(v) {
                out.push((ptr.clone(), name));
                return;
            }
            let keep = ptr.len();
            match v {
                ParamValue::Array(items) => {
                    for (i, item) in items.iter().enumerate() {
                        ptr.push('/');
                        ptr.push_str(&i.to_string());
                        walk(item, ptr, out);
                        ptr.truncate(keep);
                    }
                }
                ParamValue::Object(map) => {
                    for (k, item) in map {
                        ptr.push('/');
                        ptr.push_str(&escape_token(k));
                        walk(item, ptr, out);
                        ptr.truncate(keep);
                    }
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        walk(&self.0, &mut String::new(), &mut out);
        out
    }

    /// The value with every reference replaced by its argument. Substituted values are not
    /// scanned again.
    ///
    /// # Errors
    /// [`UnboundParam`] for the first reference (in [`Self::references`] order) with no argument.
    pub fn substitute(
        &self,
        args: &BTreeMap<String, ParamValue>,
    ) -> Result<ParamValue, UnboundParam> {
        fn go(
            v: &ParamValue,
            ptr: &mut String,
            args: &BTreeMap<String, ParamValue>,
        ) -> Result<ParamValue, UnboundParam> {
            if let Some(name) = reference_name(v) {
                return args.get(name).cloned().ok_or_else(|| UnboundParam {
                    pointer: ptr.clone(),
                    name: name.to_owned(),
                });
            }
            let keep = ptr.len();
            match v {
                ParamValue::Array(items) => {
                    let mut out = Vec::with_capacity(items.len());
                    for (i, item) in items.iter().enumerate() {
                        ptr.push('/');
                        ptr.push_str(&i.to_string());
                        out.push(go(item, ptr, args)?);
                        ptr.truncate(keep);
                    }
                    Ok(ParamValue::Array(out))
                }
                ParamValue::Object(map) => {
                    let mut out = serde_json::Map::new();
                    for (k, item) in map {
                        ptr.push('/');
                        ptr.push_str(&escape_token(k));
                        out.insert(k.clone(), go(item, ptr, args)?);
                        ptr.truncate(keep);
                    }
                    Ok(ParamValue::Object(out))
                }
                other => Ok(other.clone()),
            }
        }
        go(&self.0, &mut String::new(), args)
    }
}

/// A finite state machine node (ADR 0013).
#[derive(Debug, Clone, PartialEq)]
pub struct FsmDef {
    /// Index into `states` of the initial state.
    pub initial: usize,
    /// States in file order.
    pub states: Vec<StateDef>,
    /// Named exits in file order.
    pub exits: Vec<ExitDef>,
    /// Transitions in file order (evaluation order).
    pub transitions: Vec<TransitionDef>,
}

/// An FSM state: a name and the node it runs.
#[derive(Debug, Clone, PartialEq)]
pub struct StateDef {
    /// State name.
    pub name: String,
    /// The node ticked while the state is current.
    pub node: NodeId,
}

/// A named FSM exit.
#[derive(Debug, Clone, PartialEq)]
pub struct ExitDef {
    /// Exit name.
    pub name: String,
    /// `Success` or `Failure`.
    pub result: Status,
}

/// Where a transition goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransitionTarget {
    /// Index into `states`.
    State(usize),
    /// Index into `exits`.
    Exit(usize),
}

/// What fires a transition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Trigger {
    /// A guard node that returns Success.
    When(NodeId),
    /// The current state's node finished with this status.
    On(Status),
}

/// One FSM transition.
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionDef {
    /// Index into `states`.
    pub from: usize,
    /// Target state or exit.
    pub to: TransitionTarget,
    /// What fires it.
    pub trigger: Trigger,
}

/// A v1 node type (REQ-B-7) with documentation and settings schema (catalog, editors).
#[derive(Debug, Clone, Copy)]
pub struct NodeTypeInfo {
    /// Node type name as written in graph files.
    pub name: &'static str,
    /// Human-readable description.
    pub description: &'static str,
    /// JSON Schema (text) of the node's settings, with `examples`.
    pub settings_schema: &'static str,
}

macro_rules! node_type {
    ($name:literal, $desc:literal) => {
        NodeTypeInfo {
            name: $name,
            description: $desc,
            settings_schema: include_str!(concat!("../schemas/nodes/", $name, ".json")),
        }
    };
}

/// The v1 node types in documentation order.
pub const NODE_TYPES: &[NodeTypeInfo] = &[
    node_type!(
        "sequence",
        "Ticks children in order; Failure or Running of a child ends the tick, Success when all succeed. Reactive unless memory is set."
    ),
    node_type!(
        "selector",
        "Ticks children in order; Success or Running of a child ends the tick, Failure when all fail. Reactive unless memory is set."
    ),
    node_type!(
        "parallel",
        "Ticks every unfinished child each tick; the policy decides when it finishes."
    ),
    node_type!(
        "inverter",
        "Swaps the child's Success and Failure; Running passes through."
    ),
    node_type!(
        "succeed_always",
        "Reports the child's Failure as Success; Running passes through."
    ),
    node_type!(
        "repeat",
        "Ticks its child at most once per tick, repeating it until count successes (forever when absent)."
    ),
    node_type!(
        "cooldown",
        "After the child finishes, fails without ticking it until the duration has elapsed."
    ),
    node_type!(
        "timeout",
        "Halts the child and fails when it is still Running after the duration."
    ),
    node_type!(
        "condition",
        "Leaf that checks a catalog condition; returns Success or Failure, never Running."
    ),
    node_type!(
        "action",
        "Leaf that runs a catalog action; may return Success, Failure or Running."
    ),
    node_type!(
        "subgraph",
        "Runs another behavior graph with its own blackboard, bound by interface parameters and outputs."
    ),
    node_type!(
        "fsm",
        "Finite state machine whose states are nodes; guard and completion transitions, named exits mapped to Success or Failure."
    ),
];
