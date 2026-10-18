//! `[nodes.*]`: the node tables and the settings of every node type except `fsm` (ADR 0012,
//! REQ-B-7). Nodes become `NodeDef`s in file order; a child name resolves to a `NodeId` here, an
//! unknown name is `unknown_node` (shape checks over the whole tree are WI-13).

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use cgf_core::{
    ComponentId, Composite, Decorator, LeafRef, NODE_TYPES, NodeDef, NodeId, NodeKind, ParallelDef,
    ParallelPolicy, RepeatDef, Seconds, SubgraphRef, TimedDef, VersionReq,
};

use super::params::{self, NodeParams};
use super::{check_name, fsm, shown};
use crate::diagnostic::DiagnosticCode;
use crate::reader::Reader;
use crate::value::{LEntry, LTable, LValue};

/// The nodes of a graph and their names.
pub(super) struct NodeTable {
    /// In file order; `NodeId` indexes this vector. A node whose table has an error is `None`, so
    /// the checks that need only the good nodes can still run on a file with errors (ASM-056).
    pub(super) slots: Vec<(String, Option<NodeDef>)>,
    /// Node name to id.
    pub(super) index: BTreeMap<String, NodeId>,
}

impl NodeTable {
    /// The node definitions when every node was read.
    pub(super) fn defs(&self) -> Option<Vec<NodeDef>> {
        self.slots.iter().map(|(_, def)| def.clone()).collect()
    }
}

/// The reading state shared by the node readers.
pub(super) struct Cx<'a, 'f> {
    pub(super) reader: &'a mut Reader<'f>,
    index: &'a BTreeMap<String, NodeId>,
    /// The node being read, for messages.
    node: String,
}

/// Reads `[nodes.*]` of the graph table. `None` when `nodes` is missing or not a table.
pub(super) fn read(reader: &mut Reader<'_>, graph: &LTable) -> Option<NodeTable> {
    let Some(value) = graph.get("nodes") else {
        let message = "missing required key `nodes`".to_string();
        reader.error(DiagnosticCode::MissingKey, message, graph.origin());
        return None;
    };
    let table = reader.table("nodes", value)?;
    let entries = in_file_order(table);
    let mut index = BTreeMap::new();
    for (i, (name, entry)) in entries.iter().enumerate() {
        check_name(reader, name, "node name", entry.key_origin());
        let id = NodeId(u32::try_from(i).unwrap_or(u32::MAX));
        index.insert((*name).to_string(), id);
    }
    let mut slots = Vec::new();
    let mut cx = Cx {
        reader,
        index: &index,
        node: String::new(),
    };
    for (name, entry) in &entries {
        let def = cx.node(name, entry.value());
        slots.push(((*name).to_string(), def));
    }
    Some(NodeTable { slots, index })
}

/// The entries of `table` in the order they are written (tables iterate by key).
pub(super) fn in_file_order(table: &LTable) -> Vec<(&str, &LEntry)> {
    let mut entries: Vec<_> = table.iter().collect();
    entries.sort_by_key(|(_, entry)| entry.key_origin().span().start);
    entries
}

impl Cx<'_, '_> {
    fn node(&mut self, name: &str, value: &LValue) -> Option<NodeDef> {
        self.node = shown(name);
        let table = self.reader.table(&format!("nodes.{}", self.node), value)?;
        let type_value = self.require(table, "type")?;
        let text = self.reader.string("type", type_value)?;
        let kind = match text {
            "sequence" => NodeKind::Sequence(self.composite(table)?),
            "selector" => NodeKind::Selector(self.composite(table)?),
            "parallel" => NodeKind::Parallel(self.parallel(table)?),
            "inverter" => NodeKind::Inverter(self.decorator(table)?),
            "succeed_always" => NodeKind::SucceedAlways(self.decorator(table)?),
            "repeat" => NodeKind::Repeat(self.repeat(table)?),
            "cooldown" => NodeKind::Cooldown(self.timed(table)?),
            "timeout" => NodeKind::Timeout(self.timed(table)?),
            "condition" => NodeKind::Condition(self.leaf(table)?),
            "action" => NodeKind::Action(self.leaf(table)?),
            "subgraph" => NodeKind::Subgraph(self.subgraph(table)?),
            "fsm" => NodeKind::Fsm(fsm::read(self, table)?),
            other => {
                let known: Vec<&str> = NODE_TYPES.iter().map(|t| t.name).collect();
                let message = format!(
                    "unknown node type `{}` (use one of: {})",
                    shown(other),
                    known.join(", ")
                );
                self.reader
                    .error(DiagnosticCode::InvalidValue, message, type_value.origin());
                return None;
            }
        };
        Some(NodeDef::new(name, kind))
    }

    /// The value of a required setting, or `missing_key` (located at the node table).
    pub(super) fn require<'t>(&mut self, table: &'t LTable, key: &str) -> Option<&'t LValue> {
        let found = table.get(key);
        if found.is_none() {
            let message = format!("node `{}` is missing required key `{key}`", self.node);
            self.reader
                .error(DiagnosticCode::MissingKey, message, table.origin());
        }
        found
    }

    /// Reports keys other than `type` and `settings` as `unknown_key`.
    pub(super) fn known(&mut self, table: &LTable, settings: &[&str]) {
        let mut keys = vec!["type"];
        keys.extend_from_slice(settings);
        self.reader.unknown_keys(table, &keys);
    }

    /// A node name written in `setting`, resolved; an unknown name is `unknown_node`.
    pub(super) fn node_ref(&mut self, value: &LValue, setting: &str) -> Option<NodeId> {
        let text = self.reader.string(setting, value)?;
        let found = self.index.get(text).copied();
        if found.is_none() {
            let message = format!(
                "`{setting}` names node `{0}`, which no `[nodes.{0}]` table defines",
                shown(text)
            );
            self.reader
                .error(DiagnosticCode::UnknownNode, message, value.origin());
        }
        found
    }

    fn children(&mut self, table: &LTable) -> Option<Vec<NodeId>> {
        let value = self.require(table, "children")?;
        let items = self.reader.array("children", value)?;
        if items.is_empty() {
            let message = "`children` must list at least one node".to_string();
            self.reader
                .error(DiagnosticCode::InvalidValue, message, value.origin());
            return None;
        }
        let ids: Vec<Option<NodeId>> = items
            .iter()
            .map(|item| self.node_ref(item, "children"))
            .collect();
        ids.into_iter().collect()
    }

    fn child(&mut self, table: &LTable) -> Option<NodeId> {
        let value = self.require(table, "child")?;
        self.node_ref(value, "child")
    }

    fn composite(&mut self, table: &LTable) -> Option<Composite> {
        self.known(table, &["children", "memory"]);
        let children = self.children(table);
        let memory = table
            .get("memory")
            .map_or(Some(false), |v| self.reader.boolean("memory", v));
        Some(Composite::new(children?).with_memory(memory?))
    }

    fn parallel(&mut self, table: &LTable) -> Option<ParallelDef> {
        self.known(table, &["children", "policy"]);
        let children = self.children(table);
        let policy = table
            .get("policy")
            .map_or(Some(ParallelPolicy::All), |v| self.policy(v));
        Some(ParallelDef::new(children?).with_policy(policy?))
    }

    fn policy(&mut self, value: &LValue) -> Option<ParallelPolicy> {
        match self.reader.string("policy", value)? {
            "all" => Some(ParallelPolicy::All),
            "any" => Some(ParallelPolicy::Any),
            other => {
                let message = format!(
                    "`policy` must be \"all\" or \"any\", found `{}`",
                    shown(other)
                );
                self.reader
                    .error(DiagnosticCode::InvalidValue, message, value.origin());
                None
            }
        }
    }

    fn decorator(&mut self, table: &LTable) -> Option<Decorator> {
        self.known(table, &["child"]);
        Some(Decorator::new(self.child(table)?))
    }

    fn repeat(&mut self, table: &LTable) -> Option<RepeatDef> {
        self.known(table, &["child", "count"]);
        let child = self.child(table);
        let count = table
            .get("count")
            .map_or(Some(None), |v| self.count(v).map(Some));
        let repeat = RepeatDef::new(child?);
        Some(match count? {
            Some(n) => repeat.with_count(n),
            None => repeat,
        })
    }

    /// `count`: an integer of at least 1 that fits in 32 bits.
    fn count(&mut self, value: &LValue) -> Option<NonZeroU32> {
        let n = self.reader.integer("count", value)?;
        let count = u32::try_from(n).ok().and_then(NonZeroU32::new);
        if count.is_none() {
            let message = format!("`count` must be between 1 and {}, found {n}", u32::MAX);
            self.reader
                .error(DiagnosticCode::InvalidValue, message, value.origin());
        }
        count
    }

    fn timed(&mut self, table: &LTable) -> Option<TimedDef> {
        self.known(table, &["child", "duration_s"]);
        let child = self.child(table);
        let duration = self
            .require(table, "duration_s")
            .and_then(|v| self.duration(v));
        Some(TimedDef::new(child?, duration?))
    }

    /// `duration_s`: a number greater than zero.
    fn duration(&mut self, value: &LValue) -> Option<Seconds> {
        let seconds = self.reader.number("duration_s", value)?;
        if seconds > 0.0 {
            return Some(Seconds(seconds));
        }
        let message = format!("`duration_s` must be greater than 0, found {seconds}");
        self.reader
            .error(DiagnosticCode::InvalidValue, message, value.origin());
        None
    }

    fn component_ref(&mut self, table: &LTable) -> Option<ComponentId> {
        let value = self.require(table, "ref")?;
        self.reader.id("ref", value, ComponentId::new)
    }

    fn leaf(&mut self, table: &LTable) -> Option<LeafRef> {
        self.known(table, &["ref", "params"]);
        let id = self.component_ref(table);
        let params = self.params(table, false);
        Some(LeafRef::new(id?, params?.template))
    }

    fn params(&mut self, table: &LTable, inputs_allowed: bool) -> Option<NodeParams> {
        table.get("params").map_or_else(
            || Some(NodeParams::none()),
            |v| params::read(self.reader, v, inputs_allowed),
        )
    }

    fn subgraph(&mut self, table: &LTable) -> Option<SubgraphRef> {
        self.known(table, &["ref", "version", "params", "outputs"]);
        let id = self.component_ref(table);
        let version = table
            .get("version")
            .map_or(Some(None), |v| self.version_req(v).map(Some));
        let params = self.params(table, true);
        let outputs = table
            .get("outputs")
            .map_or_else(|| Some(BTreeMap::new()), |v| self.outputs(v));
        let params = params?;
        let mut call = SubgraphRef::new(id?, params.template);
        if let Some(req) = version? {
            call = call.with_version(req);
        }
        for (param, key) in params.inputs {
            call = call.with_input(param, key);
        }
        for (port, key) in outputs? {
            call = call.with_output(port, key);
        }
        Some(call)
    }

    /// `version`: a semver range; `invalid_version` otherwise.
    fn version_req(&mut self, value: &LValue) -> Option<VersionReq> {
        let text = self.reader.string("version", value)?;
        match VersionReq::parse(text) {
            Ok(req) => Some(req),
            Err(error) => {
                let message = format!(
                    "`version` {:?} is not a semantic version range: {error}",
                    shown(text)
                );
                self.reader
                    .error(DiagnosticCode::InvalidVersion, message, value.origin());
                None
            }
        }
    }

    /// Call-site `outputs = { <port> = "<parent key>" }`.
    fn outputs(&mut self, value: &LValue) -> Option<BTreeMap<String, String>> {
        let table = self.reader.table("outputs", value)?;
        let mut out = BTreeMap::new();
        let mut ok = true;
        for (port, entry) in table.iter() {
            ok &= check_name(self.reader, port, "output port", entry.key_origin());
            let key = entry.value();
            match self.reader.string("outputs", key) {
                Some(parent) if check_name(self.reader, parent, "blackboard key", key.origin()) => {
                    out.insert(port.to_string(), parent.to_string());
                }
                _ => ok = false,
            }
        }
        ok.then_some(out)
    }
}
