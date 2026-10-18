//! The factory that turns a [`GraphDef`] into a [`Behavior`] (ADR 0011, ADR 0014).
//!
//! The node types of ADR 0013 are built except `subgraph` (WI-10): a graph with one is refused
//! with [`GraphRegistrationError::Unsupported`].

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use cgf_core::{
    Action, Behavior, BehaviorId, BuildError, ComponentId, Condition, Factory, GraphDef, Interface,
    LeafRef, NodeDef, NodeKind, ParamSchema, ParamValue, Registry, RegistryError,
};

use crate::instance::GraphBehavior;
use crate::program::{self, Kind, Node, Program};

/// Why a graph could not be registered or built into a behavior.
///
/// Inputs have passed the loader, so these are bugs or a registry that changed between load and
/// instantiate.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GraphRegistrationError {
    /// The registry refused the graph (for example a duplicate ID).
    #[error(transparent)]
    Registry(#[from] RegistryError),
    /// A leaf or subgraph the graph names is not registered.
    #[error("graph `{graph}`: `{id}` is not registered")]
    Unresolved {
        /// The graph with the dangling reference.
        graph: BehaviorId,
        /// The missing component.
        id: ComponentId,
    },
    /// The graph is part of a subgraph cycle.
    #[error("graph `{graph}` is part of a subgraph cycle")]
    Cycle {
        /// A graph on the cycle.
        graph: BehaviorId,
    },
    /// A leaf or child factory rejected the graph's parameters.
    #[error("graph `{graph}`: {source}")]
    Build {
        /// The graph being built.
        graph: BehaviorId,
        /// The factory's error.
        #[source]
        source: BuildError,
    },
    /// The node arena is not a tree of valid nodes (an index out of range, a node reached twice,
    /// nesting deeper than 128 levels in one graph). The loader reports the first two with
    /// locations; it does not limit node depth yet, so this is where deep nesting is refused.
    #[error("graph `{graph}`: {reason}")]
    Malformed {
        /// The graph with the problem.
        graph: BehaviorId,
        /// What is wrong.
        reason: String,
    },
    /// The graph uses a node type this build of the interpreter does not run yet.
    #[error("graph `{graph}`: node `{node}` has a type that is not supported yet")]
    Unsupported {
        /// The graph with the node.
        graph: BehaviorId,
        /// The node's name.
        node: String,
    },
}

/// A leaf factory resolved at construction.
enum Leaf {
    None,
    Condition(Arc<dyn Factory<dyn Condition>>),
    Action(Arc<dyn Factory<dyn Action>>),
}

/// Builds a [`Behavior`] from a [`GraphDef`]: merges defaults, substitutes `{ param }` references,
/// and builds leaves and child behaviors through the registry.
pub struct GraphFactory {
    id: ComponentId,
    def: Arc<GraphDef>,
    leaves: Vec<Leaf>,
}

impl fmt::Debug for GraphFactory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GraphFactory")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl GraphFactory {
    /// A factory for `def`, with every leaf and subgraph resolved in `registry`.
    ///
    /// # Errors
    /// [`GraphRegistrationError::Unresolved`] for a reference the registry does not know;
    /// [`GraphRegistrationError::Malformed`] and [`GraphRegistrationError::Unsupported`] for a
    /// node arena the interpreter cannot run.
    pub fn new(def: Arc<GraphDef>, registry: &Registry) -> Result<Self, GraphRegistrationError> {
        program::validate(&def)?;
        let id =
            ComponentId::new(def.id.as_str()).map_err(|e| GraphRegistrationError::Malformed {
                graph: def.id.clone(),
                reason: e.to_string(),
            })?;
        let leaves = def
            .nodes
            .iter()
            .map(|node| resolve(&def, node, registry))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { id, def, leaves })
    }
}

fn resolve(
    def: &GraphDef,
    node: &NodeDef,
    registry: &Registry,
) -> Result<Leaf, GraphRegistrationError> {
    let unresolved = |leaf: &LeafRef| GraphRegistrationError::Unresolved {
        graph: def.id.clone(),
        id: leaf.id.clone(),
    };
    match &node.kind {
        NodeKind::Condition(leaf) => registry
            .condition(&leaf.id)
            .map(|f| Leaf::Condition(Arc::clone(f)))
            .ok_or_else(|| unresolved(leaf)),
        NodeKind::Action(leaf) => registry
            .action(&leaf.id)
            .map(|f| Leaf::Action(Arc::clone(f)))
            .ok_or_else(|| unresolved(leaf)),
        _ => Ok(Leaf::None),
    }
}

/// The interface defaults overlaid with the caller's `params`.
fn merge_args(
    interface: &Interface,
    params: &ParamValue,
) -> Result<BTreeMap<String, ParamValue>, BuildError> {
    let mut args = interface.defaults.clone();
    match params {
        ParamValue::Null => {}
        ParamValue::Object(map) => args.extend(map.iter().map(|(k, v)| (k.clone(), v.clone()))),
        _ => return Err(BuildError::new("parameters must be an object")),
    }
    Ok(args)
}

impl GraphFactory {
    fn leaf_error(node: &NodeDef, e: &BuildError) -> BuildError {
        BuildError::new(format!("node `{}`: {}", node.name, e.message))
    }

    fn kind(
        node: &NodeDef,
        leaf: &Leaf,
        args: &BTreeMap<String, ParamValue>,
    ) -> Result<Kind, BuildError> {
        let fail = || BuildError::new(format!("node `{}` cannot be built", node.name));
        let ix = program::index;
        Ok(match (&node.kind, leaf) {
            (NodeKind::Condition(r), Leaf::Condition(f)) => {
                let params = r
                    .params
                    .substitute(args)
                    .map_err(|e| BuildError::new(format!("node `{}`: {e}", node.name)))?;
                Kind::Condition(f.build(&params).map_err(|e| Self::leaf_error(node, &e))?)
            }
            (NodeKind::Action(r), Leaf::Action(f)) => {
                let params = r
                    .params
                    .substitute(args)
                    .map_err(|e| BuildError::new(format!("node `{}`: {e}", node.name)))?;
                Kind::Action(f.build(&params).map_err(|e| Self::leaf_error(node, &e))?)
            }
            (NodeKind::Sequence(c), _) => Kind::Sequence {
                children: c.children.iter().map(|&n| ix(n)).collect(),
                memory: c.memory,
            },
            (NodeKind::Selector(c), _) => Kind::Selector {
                children: c.children.iter().map(|&n| ix(n)).collect(),
                memory: c.memory,
            },
            (NodeKind::Parallel(p), _) => Kind::Parallel {
                children: p.children.iter().map(|&n| ix(n)).collect(),
                policy: program::policy(p.policy).ok_or_else(fail)?,
            },
            (NodeKind::Inverter(d), _) => Kind::Inverter(ix(d.child)),
            (NodeKind::SucceedAlways(d), _) => Kind::SucceedAlways(ix(d.child)),
            (NodeKind::Repeat(r), _) => Kind::Repeat {
                child: ix(r.child),
                count: r.count.map(std::num::NonZeroU32::get),
            },
            (NodeKind::Cooldown(t), _) => Kind::Cooldown {
                child: ix(t.child),
                duration: t.duration,
            },
            (NodeKind::Timeout(t), _) => Kind::Timeout {
                child: ix(t.child),
                duration: t.duration,
            },
            (NodeKind::Fsm(def), _) => Kind::Fsm(program::fsm(def).ok_or_else(fail)?),
            _ => return Err(fail()),
        })
    }
}

impl Factory<dyn Behavior> for GraphFactory {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn param_schema(&self) -> &ParamSchema {
        &self.def.interface.params
    }

    fn build(&self, params: &ParamValue) -> Result<Arc<dyn Behavior>, BuildError> {
        let def = &self.def;
        let args = merge_args(&def.interface, params)?;
        let nodes = def
            .nodes
            .iter()
            .zip(&self.leaves)
            .map(|(node, leaf)| {
                Ok(Node {
                    name: node.name.as_str().into(),
                    kind: Self::kind(node, leaf, &args)?,
                })
            })
            .collect::<Result<Vec<_>, BuildError>>()?;
        let program = Program {
            nodes,
            root: program::index(def.root),
            blackboard: def.blackboard.clone(),
            inputs: def.interface.types.clone(),
            outputs: def
                .interface
                .outputs
                .iter()
                .map(|(port, out)| (port.clone(), out.from.clone()))
                .collect(),
        };
        Ok(Arc::new(GraphBehavior {
            id: def.id.clone(),
            program: Arc::new(program),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cgf_core::{
        Composite, Decorator, ExitDef, FsmDef, NodeId, ParamSchema, ParamTemplate, Provenance,
        StateDef, Status, TransitionDef, TransitionTarget, Trigger, Version,
    };

    fn graph(root: u32, nodes: Vec<NodeDef>) -> Arc<GraphDef> {
        let schema =
            ParamSchema::from_json_str(r#"{"type":"object"}"#).unwrap_or_else(|e| panic!("{e}"));
        let id = BehaviorId::new("test.graph.shape").unwrap_or_else(|e| panic!("{e}"));
        Arc::new(GraphDef::new(
            id,
            Version::new(1, 0, 0),
            Provenance::File,
            Interface::new(schema),
            NodeId(root),
            nodes,
        ))
    }

    fn seq(children: &[u32]) -> NodeDef {
        let ids = children.iter().map(|&c| NodeId(c)).collect();
        NodeDef::new("s", NodeKind::Sequence(Composite::new(ids)))
    }

    fn reason(def: Arc<GraphDef>) -> Option<GraphRegistrationError> {
        GraphFactory::new(def, &Registry::new()).err()
    }

    #[test]
    fn root_or_child_out_of_range_is_malformed() {
        assert!(matches!(
            reason(graph(3, vec![seq(&[])])),
            Some(GraphRegistrationError::Malformed { .. })
        ));
        assert!(matches!(
            reason(graph(0, vec![seq(&[9])])),
            Some(GraphRegistrationError::Malformed { .. })
        ));
    }

    #[test]
    fn a_cycle_or_shared_node_is_malformed() {
        assert!(matches!(
            reason(graph(0, vec![seq(&[0])])),
            Some(GraphRegistrationError::Malformed { .. })
        ));
        assert!(matches!(
            reason(graph(0, vec![seq(&[1, 1]), seq(&[])])),
            Some(GraphRegistrationError::Malformed { .. })
        ));
    }

    #[test]
    fn nesting_beyond_the_limit_is_malformed_and_at_the_limit_is_fine() {
        let chain = |depth: u32| {
            let mut nodes: Vec<NodeDef> = (1..depth)
                .map(|i| NodeDef::new("d", NodeKind::Inverter(Decorator::new(NodeId(i)))))
                .collect();
            nodes.push(seq(&[]));
            graph(0, nodes)
        };
        let limit = u32::try_from(program::MAX_NODE_DEPTH).unwrap_or(u32::MAX);
        assert!(reason(chain(limit)).is_none());
        assert!(matches!(
            reason(chain(limit + 1)),
            Some(GraphRegistrationError::Malformed { .. })
        ));
    }

    fn fsm_graph(def: FsmDef, extra: Vec<NodeDef>) -> Arc<GraphDef> {
        let mut nodes = vec![NodeDef::new("f", NodeKind::Fsm(def))];
        nodes.extend(extra);
        graph(0, nodes)
    }

    fn one_state(initial: usize) -> FsmDef {
        FsmDef::new(initial, vec![StateDef::new("a", NodeId(1))])
    }

    #[test]
    fn an_fsm_with_a_bad_shape_is_malformed_and_a_good_one_is_fine() {
        let malformed = |def: FsmDef| {
            matches!(
                reason(fsm_graph(def, vec![seq(&[])])),
                Some(GraphRegistrationError::Malformed { .. })
            )
        };
        assert!(reason(fsm_graph(one_state(0), vec![seq(&[])])).is_none());
        assert!(malformed(one_state(1)));
        assert!(malformed(FsmDef::new(
            0,
            vec![StateDef::new("a", NodeId(9))]
        )));
        assert!(malformed(
            one_state(0).with_exit(ExitDef::new("x", Status::Running))
        ));
        assert!(malformed(one_state(0).with_transition(TransitionDef::new(
            0,
            TransitionTarget::State(0),
            Trigger::On(Status::Running)
        ))));
        for (from, to) in [
            (5, TransitionTarget::State(0)),
            (0, TransitionTarget::State(5)),
            (0, TransitionTarget::Exit(0)),
        ] {
            let t = TransitionDef::new(from, to, Trigger::On(Status::Success));
            assert!(malformed(one_state(0).with_transition(t)), "{from} {to:?}");
        }
    }

    #[test]
    fn a_cyclic_or_shared_guard_is_malformed_and_terminates() {
        let guarded = |nodes: Vec<NodeDef>| {
            let def = one_state(0).with_transition(TransitionDef::new(
                0,
                TransitionTarget::State(0),
                Trigger::When(NodeId(2)),
            ));
            let mut all = vec![NodeDef::new("f", NodeKind::Fsm(def)), seq(&[])];
            all.extend(nodes);
            reason(graph(0, all))
        };
        // g = sequence [g]
        assert!(matches!(
            guarded(vec![seq(&[2])]),
            Some(GraphRegistrationError::Malformed { .. })
        ));
        // g = sequence [c, c]
        assert!(matches!(
            guarded(vec![seq(&[3, 3]), seq(&[])]),
            Some(GraphRegistrationError::Malformed { .. })
        ));
    }

    #[test]
    fn an_action_inside_a_guard_is_malformed() {
        let action = NodeDef::new(
            "act",
            NodeKind::Action(LeafRef::new(
                ComponentId::new("stub.action").unwrap_or_else(|e| panic!("{e}")),
                ParamTemplate::new(ParamValue::Null),
            )),
        );
        let guarded = |guard: NodeDef| {
            let def = one_state(0).with_transition(TransitionDef::new(
                0,
                TransitionTarget::State(0),
                Trigger::When(NodeId(2)),
            ));
            reason(fsm_graph(def, vec![seq(&[]), guard]))
        };
        // The action is refused by the guard check before leaf resolution.
        let err = guarded(action);
        assert!(
            matches!(&err, Some(GraphRegistrationError::Malformed { reason, .. }) if reason.contains("guard `act`")),
            "{err:?}"
        );
        // A sequence guard that contains an action is refused too.
        let nested = NodeDef::new("g", NodeKind::Sequence(Composite::new(vec![NodeId(3)])));
        let def = one_state(0).with_transition(TransitionDef::new(
            0,
            TransitionTarget::State(0),
            Trigger::When(NodeId(2)),
        ));
        let action = NodeDef::new(
            "act",
            NodeKind::Action(LeafRef::new(
                ComponentId::new("stub.action").unwrap_or_else(|e| panic!("{e}")),
                ParamTemplate::new(ParamValue::Null),
            )),
        );
        assert!(matches!(
            reason(fsm_graph(def, vec![seq(&[]), nested, action])),
            Some(GraphRegistrationError::Malformed { .. })
        ));
    }
}
