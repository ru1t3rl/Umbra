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

/// The deepest node nesting one graph may have, the root being level 1. The interpreter recurses
/// once per level, so this bounds its stack use; the loader reports a deeper graph as
/// `node_too_deep` (one constant, so loader and interpreter cannot disagree; ASM-095 d, ASM-106).
///
/// A `usize` because it is compared with node counts and stack depths.
pub const MAX_NODE_DEPTH: usize = 128;

/// The default limit on a `subgraph` chain, counted in graphs including the first (ASM-073): the
/// default of the loader's `max_subgraph_depth` and the limit `register_graphs` applies
/// (ASM-106).
///
/// A `u32` because it is a loader option (`LoaderOptions::max_subgraph_depth`), a plain number
/// users set.
pub const DEFAULT_MAX_SUBGRAPH_DEPTH: u32 = 16;

/// Index of a node in [`GraphDef::nodes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeId(pub u32);

/// A behavior graph: tree of nodes plus interface and blackboard declarations.
///
/// Fields are public for reading; build with [`GraphDef::new`] and the `with_*` methods
/// (`#[non_exhaustive]`, so adding a field is additive, ASM-090).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
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
    /// A graph with no description, origin or blackboard keys.
    pub fn new(
        id: BehaviorId,
        version: Version,
        source: Provenance,
        interface: Interface,
        root: NodeId,
        nodes: Vec<NodeDef>,
    ) -> Self {
        Self {
            id,
            version,
            description: None,
            source,
            origin: None,
            interface,
            blackboard: BTreeMap::new(),
            root,
            nodes,
        }
    }

    /// Sets the description.
    #[must_use]
    pub fn with_description(mut self, d: impl Into<String>) -> Self {
        self.description = Some(d.into());
        self
    }

    /// Sets the file path used in messages.
    #[must_use]
    pub fn with_origin(mut self, p: impl Into<PathBuf>) -> Self {
        self.origin = Some(p.into());
        self
    }

    /// Declares a blackboard key (replaces an earlier declaration of the same key).
    #[must_use]
    pub fn with_blackboard_key(mut self, key: impl Into<String>, ty: BbType) -> Self {
        self.blackboard.insert(key.into(), ty);
        self
    }

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
#[non_exhaustive]
pub struct Interface {
    /// Object schema assembled from the literal-capable params.
    pub params: ParamSchema,
    /// Every param's blackboard type, including binding-only `entity` and `position` params.
    pub types: BTreeMap<String, BbType>,
    /// Default values of params that have one.
    pub defaults: BTreeMap<String, ParamValue>,
    /// Output ports by name.
    pub outputs: BTreeMap<String, OutputPort>,
    /// Free-text result semantics (REQ-B-10): what Success, Failure and Running mean. Not a port
    /// name.
    pub result: Option<String>,
}

impl Interface {
    /// An interface with the given params schema and nothing else declared.
    pub fn new(params: ParamSchema) -> Self {
        Self {
            params,
            types: BTreeMap::new(),
            defaults: BTreeMap::new(),
            outputs: BTreeMap::new(),
            result: None,
        }
    }

    /// Declares a param's blackboard type.
    #[must_use]
    pub fn with_type(mut self, param: impl Into<String>, ty: BbType) -> Self {
        self.types.insert(param.into(), ty);
        self
    }

    /// Sets a param's default value.
    #[must_use]
    pub fn with_default(mut self, param: impl Into<String>, v: ParamValue) -> Self {
        self.defaults.insert(param.into(), v);
        self
    }

    /// Declares an output port.
    #[must_use]
    pub fn with_output(mut self, port: impl Into<String>, out: OutputPort) -> Self {
        self.outputs.insert(port.into(), out);
        self
    }

    /// Sets the free-text result semantics.
    #[must_use]
    pub fn with_result(mut self, text: impl Into<String>) -> Self {
        self.result = Some(text.into());
        self
    }

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
#[non_exhaustive]
pub struct OutputPort {
    /// Port type.
    pub ty: BbType,
    /// Blackboard key the port is copied from.
    pub from: String,
}

impl OutputPort {
    /// A port of type `ty` copied from blackboard key `from`.
    pub fn new(ty: BbType, from: impl Into<String>) -> Self {
        Self {
            ty,
            from: from.into(),
        }
    }
}

/// A named node.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct NodeDef {
    /// Node name (`[a-z][a-z0-9_]*`), unique in the graph.
    pub name: String,
    /// Type and settings.
    pub kind: NodeKind,
}

impl NodeDef {
    /// A node called `name`.
    pub fn new(name: impl Into<String>, kind: NodeKind) -> Self {
        Self {
            name: name.into(),
            kind,
        }
    }
}

/// Node type and its settings (REQ-B-7). Settings live in structs so a new setting is an
/// additive field, not a breaking variant change (ASM-090). Children are [`NodeId`]s.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum NodeKind {
    /// Runs children in order until one fails or is Running.
    Sequence(Composite),
    /// Runs children in order until one succeeds or is Running.
    Selector(Composite),
    /// Ticks every unfinished child each tick.
    Parallel(ParallelDef),
    /// Swaps Success and Failure of the child.
    Inverter(Decorator),
    /// Maps the child's Failure to Success.
    SucceedAlways(Decorator),
    /// Repeats the child.
    Repeat(RepeatDef),
    /// Blocks the child for a duration after it finishes.
    Cooldown(TimedDef),
    /// Aborts the child when it runs longer than a duration.
    Timeout(TimedDef),
    /// A condition leaf.
    Condition(LeafRef),
    /// An action leaf.
    Action(LeafRef),
    /// Another graph run with its own blackboard.
    Subgraph(SubgraphRef),
    /// A finite state machine.
    Fsm(FsmDef),
}

/// Settings of `sequence` and `selector`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Composite {
    /// Children in order (at least one, enforced by the reader).
    pub children: Vec<NodeId>,
    /// Resume at the Running child instead of re-checking from the first.
    pub memory: bool,
}

impl Composite {
    /// Reactive composite (`memory = false`).
    pub fn new(children: Vec<NodeId>) -> Self {
        Self {
            children,
            memory: false,
        }
    }

    /// Sets `memory`.
    #[must_use]
    pub fn with_memory(mut self, memory: bool) -> Self {
        self.memory = memory;
        self
    }
}

/// Settings of `parallel`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ParallelDef {
    /// Children in order (at least one, enforced by the reader).
    pub children: Vec<NodeId>,
    /// When the node finishes.
    pub policy: ParallelPolicy,
}

impl ParallelDef {
    /// Policy [`ParallelPolicy::All`].
    pub fn new(children: Vec<NodeId>) -> Self {
        Self {
            children,
            policy: ParallelPolicy::All,
        }
    }

    /// Sets the policy.
    #[must_use]
    pub fn with_policy(mut self, policy: ParallelPolicy) -> Self {
        self.policy = policy;
        self
    }
}

/// Settings of `inverter` and `succeed_always`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Decorator {
    /// The child.
    pub child: NodeId,
}

impl Decorator {
    /// A decorator over `child`.
    pub fn new(child: NodeId) -> Self {
        Self { child }
    }
}

/// Settings of `repeat`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct RepeatDef {
    /// The child.
    pub child: NodeId,
    /// Successes needed; `None` repeats forever.
    pub count: Option<NonZeroU32>,
}

impl RepeatDef {
    /// Repeats forever.
    pub fn new(child: NodeId) -> Self {
        Self { child, count: None }
    }

    /// Stops after `count` child successes.
    #[must_use]
    pub fn with_count(mut self, count: NonZeroU32) -> Self {
        self.count = Some(count);
        self
    }
}

/// Settings of `cooldown` and `timeout`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct TimedDef {
    /// The child.
    pub child: NodeId,
    /// Length (> 0, enforced by the reader).
    pub duration: Seconds,
}

impl TimedDef {
    /// A timed decorator over `child`.
    pub fn new(child: NodeId, duration: Seconds) -> Self {
        Self { child, duration }
    }
}

impl NodeKind {
    /// The node type tag.
    pub fn tag(&self) -> NodeKindTag {
        match self {
            Self::Sequence(_) => NodeKindTag::Sequence,
            Self::Selector(_) => NodeKindTag::Selector,
            Self::Parallel(_) => NodeKindTag::Parallel,
            Self::Inverter(_) => NodeKindTag::Inverter,
            Self::SucceedAlways(_) => NodeKindTag::SucceedAlways,
            Self::Repeat(_) => NodeKindTag::Repeat,
            Self::Cooldown(_) => NodeKindTag::Cooldown,
            Self::Timeout(_) => NodeKindTag::Timeout,
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
#[non_exhaustive]
pub enum ParallelPolicy {
    /// Success when all succeeded, Failure on the first failure.
    All,
    /// Success on the first success, Failure when all failed.
    Any,
}

/// A condition or action leaf: catalog ID and parameters.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct LeafRef {
    /// Catalog ID.
    pub id: ComponentId,
    /// Parameters, possibly with `{ param }` references.
    pub params: ParamTemplate,
}

impl LeafRef {
    /// A leaf reference.
    pub fn new(id: ComponentId, params: ParamTemplate) -> Self {
        Self { id, params }
    }
}

/// A `subgraph` node.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
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

impl SubgraphRef {
    /// A reference accepting any version, with no bindings.
    pub fn new(id: ComponentId, params: ParamTemplate) -> Self {
        Self {
            id,
            version: None,
            params,
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
        }
    }

    /// Restricts the accepted versions.
    #[must_use]
    pub fn with_version(mut self, req: VersionReq) -> Self {
        self.version = Some(req);
        self
    }

    /// Binds a child param to a parent blackboard key (run time).
    #[must_use]
    pub fn with_input(mut self, param: impl Into<String>, parent_key: impl Into<String>) -> Self {
        self.inputs.insert(param.into(), parent_key.into());
        self
    }

    /// Copies a child output port to a parent blackboard key.
    #[must_use]
    pub fn with_output(mut self, port: impl Into<String>, parent_key: impl Into<String>) -> Self {
        self.outputs.insert(port.into(), parent_key.into());
        self
    }
}

/// The two reserved one-key forms (ADR 0012, ADR 0014, ASM-076). Closed by format v1: a new
/// form needs an ADR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReservedForm<'a> {
    /// `{ param = "<interface param>" }`: a build-time reference.
    Param(&'a str),
    /// `{ key = "<blackboard key>" }`: a run-time input binding.
    Key(&'a str),
}

/// Classifies `value`: `Some` iff it is a JSON object with exactly one key, `param` or `key`,
/// whose value is a string.
///
/// Callers apply it to values *inside* a `params` table only, never to the table itself
/// (ASM-089 b). It is the single classifier, used by [`ParamTemplate`] and the graph reader.
pub fn reserved_form(value: &ParamValue) -> Option<ReservedForm<'_>> {
    let map = value.as_object()?;
    if map.len() != 1 {
        return None;
    }
    let (key, v) = map.iter().next()?;
    let text = v.as_str()?;
    match key.as_str() {
        "param" => Some(ReservedForm::Param(text)),
        "key" => Some(ReservedForm::Key(text)),
        _ => None,
    }
}

/// Parameters that may hold `{ param = "<name>" }` build-time references (ASM-076).
///
/// The value is a `params` table; only values inside it are classified, so a root table
/// `{ param = "x" }` is a literal param named `param`. The reader moves the top-level
/// `{ key = .. }` values of `subgraph` params to [`SubgraphRef::inputs`] and rejects every other
/// `Key` form, so one found here is a bug and [`ParamTemplate::substitute`] reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamTemplate(ParamValue);

/// Why [`ParamTemplate::substitute`] failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SubstituteError {
    /// A `{ param }` reference has no argument.
    #[error("parameter reference `{name}` at {pointer} is not bound")]
    Unbound {
        /// JSON pointer (RFC 6901) of the reference.
        pointer: String,
        /// The referenced interface parameter.
        name: String,
    },
    /// A `{ key }` input binding where only build-time values may appear.
    #[error("input binding to `{key}` at {pointer} is not a build-time value")]
    Binding {
        /// JSON pointer (RFC 6901) of the binding.
        pointer: String,
        /// The bound blackboard key.
        key: String,
    },
}

fn escape_token(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

/// The children of a container with the pointer token of each (array index or escaped key), in
/// traversal order: array order, object keys sorted. Empty for scalars.
fn children(v: &ParamValue) -> Vec<(String, &ParamValue)> {
    match v {
        ParamValue::Array(items) => items
            .iter()
            .enumerate()
            .map(|(i, x)| (i.to_string(), x))
            .collect(),
        ParamValue::Object(map) => map.iter().map(|(k, x)| (escape_token(k), x)).collect(),
        _ => Vec::new(),
    }
}

fn collect_refs<'a>(v: &'a ParamValue, ptr: &str, out: &mut Vec<(String, &'a str)>) {
    for (token, child) in children(v) {
        let child_ptr = format!("{ptr}/{token}");
        match reserved_form(child) {
            Some(ReservedForm::Param(name)) => out.push((child_ptr, name)),
            _ => collect_refs(child, &child_ptr, out),
        }
    }
}

/// Substitutes inside a container; the container itself is never classified.
fn substitute_in(
    v: &ParamValue,
    ptr: &str,
    args: &BTreeMap<String, ParamValue>,
) -> Result<ParamValue, SubstituteError> {
    let mut done = Vec::new();
    for (token, child) in children(v) {
        let child_ptr = format!("{ptr}/{token}");
        let value = match reserved_form(child) {
            Some(ReservedForm::Param(name)) => {
                args.get(name)
                    .cloned()
                    .ok_or_else(|| SubstituteError::Unbound {
                        pointer: child_ptr.clone(),
                        name: name.to_owned(),
                    })?
            }
            Some(ReservedForm::Key(key)) => {
                return Err(SubstituteError::Binding {
                    pointer: child_ptr,
                    key: key.to_owned(),
                });
            }
            None => substitute_in(child, &child_ptr, args)?,
        };
        done.push(value);
    }
    Ok(match v {
        ParamValue::Array(_) => ParamValue::Array(done),
        ParamValue::Object(map) => ParamValue::Object(map.keys().cloned().zip(done).collect()),
        other => other.clone(),
    })
}

impl ParamTemplate {
    /// Wraps a `params` table (or any value).
    pub fn new(v: ParamValue) -> Self {
        Self(v)
    }

    /// The raw value, references included.
    pub fn as_json(&self) -> &ParamValue {
        &self.0
    }

    /// Every `Param` form below the root as (JSON pointer, parameter name). Order: depth first,
    /// array order, object keys sorted. Use this to find every unbound reference at once.
    pub fn references(&self) -> Vec<(String, &str)> {
        let mut out = Vec::new();
        collect_refs(&self.0, "", &mut out);
        out
    }

    /// The value with every `Param` form replaced by its argument. Substituted values are not
    /// scanned again.
    ///
    /// # Errors
    /// [`SubstituteError::Unbound`] for the first reference with no argument, or
    /// [`SubstituteError::Binding`] for a `Key` form, whichever comes first in traversal order.
    /// Use [`Self::references`] to list every reference.
    pub fn substitute(
        &self,
        args: &BTreeMap<String, ParamValue>,
    ) -> Result<ParamValue, SubstituteError> {
        substitute_in(&self.0, "", args)
    }
}

/// A finite state machine node (ADR 0013).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
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

impl FsmDef {
    /// An FSM starting in `states[initial]` with no exits or transitions.
    pub fn new(initial: usize, states: Vec<StateDef>) -> Self {
        Self {
            initial,
            states,
            exits: Vec::new(),
            transitions: Vec::new(),
        }
    }

    /// Appends a named exit.
    #[must_use]
    pub fn with_exit(mut self, exit: ExitDef) -> Self {
        self.exits.push(exit);
        self
    }

    /// Appends a transition (file order is evaluation order).
    #[must_use]
    pub fn with_transition(mut self, t: TransitionDef) -> Self {
        self.transitions.push(t);
        self
    }
}

/// An FSM state: a name and the node it runs.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct StateDef {
    /// State name.
    pub name: String,
    /// The node ticked while the state is current.
    pub node: NodeId,
}

impl StateDef {
    /// A state that runs `node`.
    pub fn new(name: impl Into<String>, node: NodeId) -> Self {
        Self {
            name: name.into(),
            node,
        }
    }
}

/// A named FSM exit.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ExitDef {
    /// Exit name.
    pub name: String,
    /// `Success` or `Failure`.
    pub result: Status,
}

impl ExitDef {
    /// An exit that returns `result`.
    pub fn new(name: impl Into<String>, result: Status) -> Self {
        Self {
            name: name.into(),
            result,
        }
    }
}

/// Where a transition goes.
///
/// Closed: an FSM has only states and exits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransitionTarget {
    /// Index into `states`.
    State(usize),
    /// Index into `exits`.
    Exit(usize),
}

/// What fires a transition.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Trigger {
    /// A guard node that returns Success.
    When(NodeId),
    /// The current state's node finished with this status. `Running` never appears here; the
    /// reader enforces it.
    On(Status),
}

/// One FSM transition.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct TransitionDef {
    /// Index into `states`.
    pub from: usize,
    /// Target state or exit.
    pub to: TransitionTarget,
    /// What fires it.
    pub trigger: Trigger,
}

impl TransitionDef {
    /// A transition from `states[from]`.
    pub fn new(from: usize, to: TransitionTarget, trigger: Trigger) -> Self {
        Self { from, to, trigger }
    }
}

/// A v1 node type (REQ-B-7) with documentation and settings schema (catalog, editors).
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
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
