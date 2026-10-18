//! A running graph: per-entity node state, blackboard and the depth-first walk (ADR 0013).
//!
//! All node state is allocated in [`GraphBehavior::instantiate`]; ticking and halting allocate
//! nothing (NFR-1).

use std::sync::Arc;

use cgf_core::{
    ActionInstance, BbValue, Behavior, BehaviorContext, BehaviorId, BehaviorInstance, Blackboard,
    BlackboardError, EntityView, InstantiateError, IntentBuffer, Seconds, Status, TickRate,
};

use crate::nodes::{composite, decorator};
use crate::program::{Kind, Node, Program};

/// A built graph: the shared program of one parameter binding.
pub(crate) struct GraphBehavior {
    pub id: BehaviorId,
    pub program: Arc<Program>,
}

impl Behavior for GraphBehavior {
    fn id(&self) -> &BehaviorId {
        &self.id
    }

    fn instantiate(&self, rate: TickRate) -> Result<Box<dyn BehaviorInstance>, InstantiateError> {
        let program = &self.program;
        let mut states = Vec::with_capacity(program.nodes.len());
        for node in &program.nodes {
            states.push(NodeState::new(node, rate)?);
        }
        let mut bb = Blackboard::default();
        for (key, ty) in &program.blackboard {
            bb.declare(key, *ty);
        }
        for (key, ty) in &program.inputs {
            bb.declare_input(key, *ty);
        }
        Ok(Box::new(GraphInstance {
            program: Arc::clone(program),
            states,
            bb,
        }))
    }
}

/// The state of one `parallel` child.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slot {
    /// Not started, or reset.
    Idle,
    /// Returned Running last tick.
    Running,
    /// Finished with this status; kept until the parallel finishes or is halted.
    Done(Status),
}

/// Per-entity state of one node.
pub(crate) enum NodeState {
    /// Conditions, inverters and `succeed_always` keep nothing.
    Stateless,
    Action {
        inst: Box<dyn ActionInstance>,
        running: bool,
    },
    /// `sequence` and `selector`: the position of the child that returned Running last tick.
    Composite {
        running: Option<usize>,
    },
    Parallel {
        slots: Box<[Slot]>,
    },
    Repeat {
        successes: u32,
    },
    /// `ticks` is the duration in whole ticks; `until` the absolute tick the child may run again.
    Cooldown {
        ticks: u64,
        until: Option<u64>,
    },
    /// `started` is the absolute tick the child was started at.
    Timeout {
        ticks: u64,
        started: Option<u64>,
    },
}

impl NodeState {
    fn new(node: &Node, rate: TickRate) -> Result<Self, InstantiateError> {
        let ticks = |duration: Seconds| {
            rate.ticks_for(duration)
                .map(std::num::NonZeroU64::get)
                .ok_or_else(|| InstantiateError::Duration {
                    what: format!("{} `{}`", node.kind.type_name(), node.name),
                    hz: rate.hz(),
                })
        };
        Ok(match &node.kind {
            Kind::Condition(_) | Kind::Inverter(_) | Kind::SucceedAlways(_) => Self::Stateless,
            Kind::Action(a) => Self::Action {
                inst: a.instantiate(rate)?,
                running: false,
            },
            Kind::Sequence { .. } | Kind::Selector { .. } => Self::Composite { running: None },
            Kind::Parallel { children, .. } => Self::Parallel {
                slots: vec![Slot::Idle; children.len()].into_boxed_slice(),
            },
            Kind::Repeat { .. } => Self::Repeat { successes: 0 },
            Kind::Cooldown { duration, .. } => Self::Cooldown {
                ticks: ticks(*duration)?,
                until: None,
            },
            Kind::Timeout { duration, .. } => Self::Timeout {
                ticks: ticks(*duration)?,
                started: None,
            },
        })
    }
}

/// One entity's run of a graph.
struct GraphInstance {
    program: Arc<Program>,
    states: Vec<NodeState>,
    bb: Blackboard,
}

impl BehaviorInstance for GraphInstance {
    fn tick(
        &mut self,
        ctx: &mut BehaviorContext<'_>,
        entity: &EntityView<'_>,
        out: &mut IntentBuffer<'_>,
    ) -> Status {
        let program = &*self.program;
        let mut walk = Walk {
            program,
            states: &mut self.states,
            bb: &mut self.bb,
            ctx,
            entity,
            out,
        };
        walk.tick(program.root)
    }

    fn halt(&mut self) {
        halt_node(&self.program, &mut self.states, self.program.root);
    }

    fn output(&self, port: &str) -> Option<BbValue> {
        let key = self.program.outputs.get(port)?;
        self.bb.get(key).cloned()
    }

    fn set_input(&mut self, param: &str, value: Option<BbValue>) -> Result<(), BlackboardError> {
        self.bb.input(param, value)
    }
}

/// Halts the subtree at `idx` and resets its state, timers and memory (ADR 0013). Only actions
/// that are Running are told to halt; a node that never started or already finished is left
/// alone. Needs no entity, time or output, so it is a free function.
pub(crate) fn halt_node(program: &Program, states: &mut [NodeState], idx: usize) {
    let Some(node) = program.nodes.get(idx) else {
        return;
    };
    match (&node.kind, &mut states[idx]) {
        (Kind::Action(_), NodeState::Action { inst, running }) => {
            if std::mem::take(running) {
                inst.halt();
            }
        }
        (Kind::Sequence { children, .. } | Kind::Selector { children, .. }, state) => {
            *state = NodeState::Composite { running: None };
            children.iter().for_each(|&c| halt_node(program, states, c));
        }
        (Kind::Parallel { children, .. }, NodeState::Parallel { slots }) => {
            slots.fill(Slot::Idle);
            children.iter().for_each(|&c| halt_node(program, states, c));
        }
        (Kind::Inverter(c) | Kind::SucceedAlways(c), _) => halt_node(program, states, *c),
        (Kind::Repeat { child, .. }, NodeState::Repeat { successes }) => {
            *successes = 0;
            halt_node(program, states, *child);
        }
        (Kind::Cooldown { child, .. }, NodeState::Cooldown { until, .. }) => {
            *until = None;
            halt_node(program, states, *child);
        }
        (Kind::Timeout { child, .. }, NodeState::Timeout { started, .. }) => {
            *started = None;
            halt_node(program, states, *child);
        }
        _ => {}
    }
}

/// One tick's borrowed world: the program, this entity's node state and blackboard, and the
/// tick's inputs and output.
pub(crate) struct Walk<'w, 'c, 'e, 'o> {
    pub program: &'w Program,
    pub states: &'w mut [NodeState],
    pub bb: &'w mut Blackboard,
    pub ctx: &'w mut BehaviorContext<'c>,
    pub entity: &'w EntityView<'e>,
    pub out: &'w mut IntentBuffer<'o>,
}

impl Walk<'_, '_, '_, '_> {
    /// Ticks the node at `idx` (and below) once and returns its status.
    pub(crate) fn tick(&mut self, idx: usize) -> Status {
        let program = self.program;
        let Some(node) = program.nodes.get(idx) else {
            return Status::Failure;
        };
        match &node.kind {
            Kind::Condition(c) => {
                if c.check(self.ctx, self.entity, self.bb) {
                    Status::Success
                } else {
                    Status::Failure
                }
            }
            Kind::Action(_) => self.tick_action(idx),
            Kind::Sequence { children, memory } => composite::ordered(
                self,
                idx,
                children,
                *memory,
                Status::Failure,
                Status::Success,
            ),
            Kind::Selector { children, memory } => composite::ordered(
                self,
                idx,
                children,
                *memory,
                Status::Success,
                Status::Failure,
            ),
            Kind::Parallel { children, policy } => {
                composite::parallel(self, idx, children, *policy)
            }
            Kind::Inverter(child) => decorator::inverter(self, *child),
            Kind::SucceedAlways(child) => decorator::succeed_always(self, *child),
            Kind::Repeat { child, count } => decorator::repeat(self, idx, *child, *count),
            Kind::Cooldown { child, .. } => decorator::cooldown(self, idx, *child),
            Kind::Timeout { child, .. } => decorator::timeout(self, idx, *child),
        }
    }

    /// Halts the subtree at `idx`.
    pub(crate) fn halt(&mut self, idx: usize) {
        halt_node(self.program, self.states, idx);
    }

    fn tick_action(&mut self, idx: usize) -> Status {
        let NodeState::Action { inst, running } = &mut self.states[idx] else {
            return Status::Failure;
        };
        let status = inst.tick(self.ctx, self.entity, self.bb, self.out);
        *running = status == Status::Running;
        status
    }
}
