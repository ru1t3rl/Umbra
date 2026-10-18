//! The JSON Schema subset validator (ADR 0009, REQ-T-2, REQ-T-3).
//!
//! Fail-closed: a schema that uses a keyword outside the subset does not compile. Validation
//! walks the schema in sorted key order and returns every violation, each located at the offending
//! value, sorted by position, so output is stable (REQ-K-4).

use std::cmp::Ordering;
use std::collections::BTreeMap;

use cgf_core::ParamSchema;
use serde_json::{Map, Value};

use crate::diagnostic::{Diagnostic, DiagnosticCode, Location, sort_and_dedup};
use crate::value::{LKind, LTable, LValue};

/// Keywords accepted and ignored (ADR 0009).
const ANNOTATIONS: [&str; 10] = [
    "$schema",
    "$id",
    "$comment",
    "title",
    "description",
    "default",
    "examples",
    "deprecated",
    "readOnly",
    "writeOnly",
];

/// The most nodes one validation may visit (see `reject_oversized`).
const MAX_EXPANDED_NODES: u64 = 10_000;

const DEFS_PREFIX: &str = "#/$defs/";

/// A schema compiled against the ADR 0009 subset.
#[derive(Debug, Clone)]
pub struct CompiledSchema {
    nodes: Vec<Node>,
    root: usize,
}

/// A schema outside the subset or invalid.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("unsupported or invalid JSON Schema at {pointer}: {message}")]
pub struct SchemaError {
    /// JSON pointer of the offending keyword.
    pub pointer: String,
    /// What is wrong.
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JsonType {
    String,
    Number,
    Integer,
    Boolean,
    Array,
    Object,
    Null,
}

impl JsonType {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "string" => Self::String,
            "number" => Self::Number,
            "integer" => Self::Integer,
            "boolean" => Self::Boolean,
            "array" => Self::Array,
            "object" => Self::Object,
            "null" => Self::Null,
            _ => return None,
        })
    }

    fn describe(self) -> &'static str {
        match self {
            Self::String => "a string",
            Self::Number => "a number",
            Self::Integer => "an integer",
            Self::Boolean => "a boolean",
            Self::Array => "an array",
            Self::Object => "a table",
            Self::Null => "null",
        }
    }

    fn matches(self, kind: &LKind) -> bool {
        matches!(
            (self, kind),
            (Self::String, LKind::String(_))
                | (Self::Number, LKind::Integer(_) | LKind::Float(_))
                | (Self::Integer, LKind::Integer(_))
                | (Self::Boolean, LKind::Boolean(_))
                | (Self::Array, LKind::Array(_))
                | (Self::Object, LKind::Table(_))
        )
    }
}

#[derive(Debug, Clone, Default)]
enum Additional {
    #[default]
    Any,
    Forbidden,
    Schema(usize),
}

#[derive(Debug, Clone, Default)]
struct Node {
    pointer: String,
    types: Option<Vec<JsonType>>,
    properties: BTreeMap<String, usize>,
    required: Vec<String>,
    additional: Additional,
    items: Option<usize>,
    min_items: Option<u64>,
    max_items: Option<u64>,
    enumeration: Option<Vec<Value>>,
    constant: Option<Value>,
    minimum: Option<f64>,
    maximum: Option<f64>,
    exclusive_minimum: Option<f64>,
    exclusive_maximum: Option<f64>,
    min_length: Option<u64>,
    max_length: Option<u64>,
    one_of: Vec<usize>,
    reference: Option<usize>,
}

impl Node {
    /// The nodes this one points at.
    fn edges(&self) -> Vec<usize> {
        let additional = match self.additional {
            Additional::Schema(i) => Some(i),
            _ => None,
        };
        self.properties
            .values()
            .copied()
            .chain(additional)
            .chain(self.items)
            .chain(self.one_of.iter().copied())
            .chain(self.reference)
            .collect()
    }
}

impl CompiledSchema {
    /// Compiles `schema`, failing closed on any keyword outside the subset.
    pub fn compile(schema: &ParamSchema) -> Result<Self, SchemaError> {
        let Some(root_json) = schema.as_json().as_object() else {
            return Err(error("", "the schema must be a JSON object"));
        };
        let mut compiler = Compiler {
            nodes: Vec::new(),
            defs: BTreeMap::new(),
        };
        let defs = match root_json.get("$defs") {
            None => Map::new(),
            Some(Value::Object(m)) => m.clone(),
            Some(_) => return Err(error("/$defs", "`$defs` must be an object")),
        };
        for name in defs.keys() {
            compiler.defs.insert(name.clone(), compiler.nodes.len());
            compiler.nodes.push(Node::default());
        }
        for (name, def) in &defs {
            let pointer = format!("/$defs/{}", escape(name));
            let index = compiler.defs.get(name).copied().unwrap_or(0);
            let node = compiler.node(def, &pointer, false)?;
            if let Some(slot) = compiler.nodes.get_mut(index) {
                *slot = node;
            }
        }
        let root_node = compiler.node(&Value::Object(root_json.clone()), "", true)?;
        let root = compiler.nodes.len();
        compiler.nodes.push(root_node);
        compiler.reject_cycles()?;
        compiler.reject_oversized(root)?;
        Ok(Self {
            nodes: compiler.nodes,
            root,
        })
    }

    /// Every violation of `value`, located at the offending value (or its object for a missing
    /// key), sorted by position.
    pub fn validate(&self, value: &LValue, context: &str) -> Vec<Diagnostic> {
        self.run(value, context, false)
    }

    /// Like [`Self::validate`] for an object some of whose properties are not known yet (ASM-106
    /// d). Every property present is checked against its own sub-schema (type, range, enum,
    /// unknown keys, nested `required`). Only the root object, following a root `$ref` chain, is
    /// relaxed: its `required` is not enforced, and its `oneOf` passes when at least one branch
    /// passes with that branch's own `required` relaxed too; when none does, the errors of the
    /// closest branch are reported as `validate` does.
    pub fn validate_known(&self, value: &LValue, context: &str) -> Vec<Diagnostic> {
        self.run(value, context, true)
    }

    fn run(&self, value: &LValue, context: &str, partial: bool) -> Vec<Diagnostic> {
        let mut run = Run {
            schema: self,
            context,
            partial,
            out: Vec::new(),
        };
        run.node(self.root, value, &mut Vec::new());
        sort_and_dedup(&mut run.out);
        run.out
    }
}

fn error(pointer: &str, message: impl Into<String>) -> SchemaError {
    SchemaError {
        pointer: pointer.to_string(),
        message: message.into(),
    }
}

fn escape(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

struct Compiler {
    nodes: Vec<Node>,
    defs: BTreeMap<String, usize>,
}

impl Compiler {
    fn child(&mut self, json: &Value, pointer: &str) -> Result<usize, SchemaError> {
        let node = self.node(json, pointer, false)?;
        self.nodes.push(node);
        Ok(self.nodes.len() - 1)
    }

    fn node(&mut self, json: &Value, pointer: &str, root: bool) -> Result<Node, SchemaError> {
        let Some(object) = json.as_object() else {
            return Err(error(pointer, "a schema must be a JSON object"));
        };
        let mut node = Node {
            pointer: pointer.to_string(),
            ..Node::default()
        };
        for (key, value) in object {
            let at = format!("{pointer}/{}", escape(key));
            if ANNOTATIONS.contains(&key.as_str()) || (root && key == "$defs") {
                continue;
            }
            self.keyword(&mut node, key, value, &at)?;
        }
        Ok(node)
    }

    fn keyword(
        &mut self,
        node: &mut Node,
        key: &str,
        value: &Value,
        at: &str,
    ) -> Result<(), SchemaError> {
        match key {
            "type" => node.types = Some(types(value, at)?),
            "properties" => node.properties = self.properties(value, at)?,
            "required" => node.required = strings(value, at)?,
            "additionalProperties" => node.additional = self.additional(value, at)?,
            "items" => node.items = Some(self.child(value, at)?),
            "minItems" => node.min_items = Some(count(value, at)?),
            "maxItems" => node.max_items = Some(count(value, at)?),
            "minLength" => node.min_length = Some(count(value, at)?),
            "maxLength" => node.max_length = Some(count(value, at)?),
            "enum" => node.enumeration = Some(non_empty_array(value, at)?.to_vec()),
            "const" => node.constant = Some(value.clone()),
            "minimum" => node.minimum = Some(number(value, at)?),
            "maximum" => node.maximum = Some(number(value, at)?),
            "exclusiveMinimum" => node.exclusive_minimum = Some(number(value, at)?),
            "exclusiveMaximum" => node.exclusive_maximum = Some(number(value, at)?),
            "oneOf" => node.one_of = self.one_of(value, at)?,
            "$ref" => node.reference = Some(self.reference(value, at)?),
            other => return Err(error(at, format!("unsupported keyword `{other}`"))),
        }
        Ok(())
    }

    fn properties(
        &mut self,
        value: &Value,
        at: &str,
    ) -> Result<BTreeMap<String, usize>, SchemaError> {
        let Some(map) = value.as_object() else {
            return Err(error(at, "`properties` must be an object"));
        };
        let mut out = BTreeMap::new();
        for (name, schema) in map {
            let child_at = format!("{at}/{}", escape(name));
            out.insert(name.clone(), self.child(schema, &child_at)?);
        }
        Ok(out)
    }

    fn additional(&mut self, value: &Value, at: &str) -> Result<Additional, SchemaError> {
        match value {
            Value::Bool(true) => Ok(Additional::Any),
            Value::Bool(false) => Ok(Additional::Forbidden),
            Value::Object(_) => Ok(Additional::Schema(self.child(value, at)?)),
            _ => Err(error(
                at,
                "`additionalProperties` must be a boolean or a schema",
            )),
        }
    }

    fn one_of(&mut self, value: &Value, at: &str) -> Result<Vec<usize>, SchemaError> {
        let branches = non_empty_array(value, at)?;
        let mut out = Vec::new();
        for (i, branch) in branches.iter().enumerate() {
            out.push(self.child(branch, &format!("{at}/{i}"))?);
        }
        Ok(out)
    }

    fn reference(&self, value: &Value, at: &str) -> Result<usize, SchemaError> {
        let Some(target) = value.as_str() else {
            return Err(error(at, "`$ref` must be a string"));
        };
        let Some(name) = target.strip_prefix(DEFS_PREFIX) else {
            return Err(error(
                at,
                format!(
                    "only local references `{DEFS_PREFIX}<name>` are supported, found `{target}`"
                ),
            ));
        };
        let name = name.replace("~1", "/").replace("~0", "~");
        self.defs
            .get(&name)
            .copied()
            .ok_or_else(|| error(at, format!("`{target}` is not defined")))
    }

    /// A reference cycle would make validation loop; any cycle is rejected (fail-closed).
    fn reject_cycles(&self) -> Result<(), SchemaError> {
        // 0 = unvisited, 1 = on the current path, 2 = done.
        let mut state = vec![0_u8; self.nodes.len()];
        for start in 0..self.nodes.len() {
            if state.get(start).copied() != Some(0) {
                continue;
            }
            let mut stack: Vec<(usize, Vec<usize>)> = vec![(start, self.edges_of(start))];
            set(&mut state, start, 1);
            while let Some((node, pending)) = stack.last_mut() {
                let node = *node;
                let Some(next) = pending.pop() else {
                    set(&mut state, node, 2);
                    stack.pop();
                    continue;
                };
                match state.get(next).copied() {
                    Some(1) => {
                        let pointer = self.nodes.get(next).map_or("", |n| n.pointer.as_str());
                        return Err(error(pointer, "`$ref` cycle"));
                    }
                    Some(0) => {
                        set(&mut state, next, 1);
                        stack.push((next, self.edges_of(next)));
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// Validation visits a shared definition once per use, so a small schema can expand to an
    /// exponential number of nodes (`oneOf` chains of `$ref`s). Reject schemas whose expansion
    /// exceeds [`MAX_EXPANDED_NODES`].
    fn reject_oversized(&self, root: usize) -> Result<(), SchemaError> {
        if u64::try_from(self.nodes.len()).map_or(true, |n| n > MAX_EXPANDED_NODES) {
            return Err(error("", "the schema has too many subschemas"));
        }
        let mut sizes: Vec<Option<u64>> = vec![None; self.nodes.len()];
        let size = self.expanded_size(root, &mut sizes);
        if size > MAX_EXPANDED_NODES {
            let message = format!(
                "the schema expands to more than {MAX_EXPANDED_NODES} nodes once `$ref`s are followed"
            );
            return Err(error("", message));
        }
        Ok(())
    }

    /// Nodes visited by one validation of `node`, saturating. The graph has no cycles here.
    fn expanded_size(&self, node: usize, memo: &mut [Option<u64>]) -> u64 {
        if let Some(Some(known)) = memo.get(node) {
            return *known;
        }
        let mut total: u64 = 1;
        for child in self.edges_of(node) {
            total = total.saturating_add(self.expanded_size(child, memo));
        }
        if let Some(slot) = memo.get_mut(node) {
            *slot = Some(total);
        }
        total
    }

    fn edges_of(&self, node: usize) -> Vec<usize> {
        self.nodes.get(node).map(Node::edges).unwrap_or_default()
    }
}

fn set(state: &mut [u8], index: usize, value: u8) {
    if let Some(slot) = state.get_mut(index) {
        *slot = value;
    }
}

fn types(value: &Value, at: &str) -> Result<Vec<JsonType>, SchemaError> {
    let names = match value {
        Value::String(s) => vec![s.as_str()],
        Value::Array(items) if !items.is_empty() => {
            let mut names = Vec::new();
            for item in items {
                names.push(
                    item.as_str()
                        .ok_or_else(|| error(at, "`type` names must be strings"))?,
                );
            }
            names
        }
        _ => {
            return Err(error(
                at,
                "`type` must be a string or a non-empty array of strings",
            ));
        }
    };
    names
        .into_iter()
        .map(|n| JsonType::parse(n).ok_or_else(|| error(at, format!("unknown type `{n}`"))))
        .collect()
}

fn strings(value: &Value, at: &str) -> Result<Vec<String>, SchemaError> {
    let Some(items) = value.as_array() else {
        return Err(error(at, "must be an array of strings"));
    };
    items
        .iter()
        .map(|i| {
            i.as_str()
                .map(str::to_string)
                .ok_or_else(|| error(at, "must be an array of strings"))
        })
        .collect()
}

fn non_empty_array<'a>(value: &'a Value, at: &str) -> Result<&'a [Value], SchemaError> {
    match value.as_array() {
        Some(items) if !items.is_empty() => Ok(items),
        _ => Err(error(at, "must be a non-empty array")),
    }
}

fn count(value: &Value, at: &str) -> Result<u64, SchemaError> {
    value
        .as_u64()
        .ok_or_else(|| error(at, "must be a non-negative integer"))
}

fn number(value: &Value, at: &str) -> Result<f64, SchemaError> {
    value
        .as_f64()
        .filter(|n| n.is_finite())
        .ok_or_else(|| error(at, "must be a number"))
}

/// One validation run: the violations found so far.
struct Run<'a> {
    schema: &'a CompiledSchema,
    context: &'a str,
    /// The root object may lack properties (`validate_known`).
    partial: bool,
    out: Vec<Diagnostic>,
}

impl Run<'_> {
    fn node(&mut self, index: usize, value: &LValue, path: &mut Vec<String>) {
        let Some(node) = self.schema.nodes.get(index) else {
            return;
        };
        if matches!(value.kind(), LKind::Unsupported) {
            return; // already reported as `unsupported_value`
        }
        if let Some(target) = node.reference {
            self.node(target, value, path);
        }
        self.check_type(node, value, path);
        self.check_enumeration(node, value, path);
        self.check_number(node, value, path);
        self.check_string(node, value, path);
        self.check_one_of(node, value, path);
        match value.kind() {
            LKind::Table(table) => self.check_table(node, value, table, path),
            LKind::Array(items) => self.check_array(node, value, items, path),
            _ => {}
        }
    }

    fn add(&mut self, code: DiagnosticCode, at: Location, path: &[String], what: &str) {
        let message = format!("{}: {}{what}", self.context, subject(path));
        self.out.push(Diagnostic::new(code, message, Some(at)));
    }

    fn check_type(&mut self, node: &Node, value: &LValue, path: &[String]) {
        let Some(types) = &node.types else { return };
        if types.iter().any(|t| t.matches(value.kind())) {
            return;
        }
        let wanted: Vec<&str> = types.iter().map(|t| t.describe()).collect();
        let what = format!(
            "must be {}, found {}",
            wanted.join(" or "),
            value.kind().type_name()
        );
        self.add(
            DiagnosticCode::ParamWrongType,
            value.origin().location(),
            path,
            &what,
        );
    }

    fn check_enumeration(&mut self, node: &Node, value: &LValue, path: &[String]) {
        if node.enumeration.is_none() && node.constant.is_none() {
            return;
        }
        let actual = value.to_param();
        if let Some(allowed) = &node.enumeration
            && !allowed.iter().any(|a| json_eq(a, &actual))
        {
            let list: Vec<String> = allowed.iter().map(ToString::to_string).collect();
            let what = format!("must be one of {}, found {actual}", list.join(", "));
            self.add(
                DiagnosticCode::ParamInvalid,
                value.origin().location(),
                path,
                &what,
            );
        }
        if let Some(expected) = &node.constant
            && !json_eq(expected, &actual)
        {
            let what = format!("must be {expected}, found {actual}");
            self.add(
                DiagnosticCode::ParamInvalid,
                value.origin().location(),
                path,
                &what,
            );
        }
    }

    fn check_number(&mut self, node: &Node, value: &LValue, path: &[String]) {
        let shown = match value.kind() {
            LKind::Integer(i) => i.to_string(),
            LKind::Float(f) => f.to_string(),
            _ => return,
        };
        let rules: [Rule; 4] = [
            (node.minimum, Ordering::is_lt, ">="),
            (node.maximum, Ordering::is_gt, "<="),
            (node.exclusive_minimum, Ordering::is_le, ">"),
            (node.exclusive_maximum, Ordering::is_ge, "<"),
        ];
        for (bound, violated, op) in rules {
            let Some(bound) = bound else { continue };
            if compare(value.kind(), bound).is_some_and(violated) {
                let what = format!("must be {op} {bound}, found {shown}");
                self.add(
                    DiagnosticCode::ParamOutOfRange,
                    value.origin().location(),
                    path,
                    &what,
                );
            }
        }
    }

    fn check_string(&mut self, node: &Node, value: &LValue, path: &[String]) {
        let LKind::String(s) = value.kind() else {
            return;
        };
        let length = s.chars().count() as u64;
        if let Some(min) = node.min_length
            && length < min
        {
            let what = format!("must have at least {min} characters, found {length}");
            self.add(
                DiagnosticCode::ParamOutOfRange,
                value.origin().location(),
                path,
                &what,
            );
        }
        if let Some(max) = node.max_length
            && length > max
        {
            let what = format!("must have at most {max} characters, found {length}");
            self.add(
                DiagnosticCode::ParamOutOfRange,
                value.origin().location(),
                path,
                &what,
            );
        }
    }

    fn check_one_of(&mut self, node: &Node, value: &LValue, path: &mut Vec<String>) {
        if node.one_of.is_empty() {
            return;
        }
        let mut best: Option<Vec<Diagnostic>> = None;
        let mut matching = 0_usize;
        for &branch in &node.one_of {
            let mut sub = Run {
                schema: self.schema,
                context: self.context,
                partial: self.partial,
                out: Vec::new(),
            };
            sub.node(branch, value, path);
            if sub.out.is_empty() {
                matching += 1;
            } else if best.as_ref().is_none_or(|b| sub.out.len() < b.len()) {
                best = Some(sub.out);
            }
        }
        if matching == 1 || (matching > 1 && self.partial && path.is_empty()) {
            return;
        }
        if matching == 0 {
            self.out.extend(best.unwrap_or_default());
        } else {
            let what = format!("must match exactly one alternative, but matches {matching}");
            self.add(
                DiagnosticCode::ParamInvalid,
                value.origin().location(),
                path,
                &what,
            );
        }
    }

    fn check_table(&mut self, node: &Node, value: &LValue, table: &LTable, path: &mut Vec<String>) {
        let relaxed = self.partial && path.is_empty();
        for name in node.required.iter().filter(|_| !relaxed) {
            if table.get(name).is_none() {
                let what = format!("missing required parameter `{}`", dotted(path, name));
                let message = format!("{}: {what}", self.context);
                let location = value.origin().location();
                self.out.push(Diagnostic::new(
                    DiagnosticCode::MissingParam,
                    message,
                    Some(location),
                ));
            }
        }
        for (key, entry) in table.iter() {
            path.push(key.to_string());
            if let Some(&schema) = node.properties.get(key) {
                self.node(schema, entry.value(), path);
            } else {
                match node.additional {
                    Additional::Any => {}
                    Additional::Forbidden => {
                        let message =
                            format!("{}: unknown parameter `{}`", self.context, path.join("."));
                        let location = entry.key_origin().location();
                        self.out.push(Diagnostic::new(
                            DiagnosticCode::UnknownParam,
                            message,
                            Some(location),
                        ));
                    }
                    Additional::Schema(schema) => self.node(schema, entry.value(), path),
                }
            }
            path.pop();
        }
    }

    fn check_array(
        &mut self,
        node: &Node,
        value: &LValue,
        items: &[LValue],
        path: &mut Vec<String>,
    ) {
        let length = items.len() as u64;
        if let Some(min) = node.min_items
            && length < min
        {
            let what = format!("must have at least {min} items, found {length}");
            self.add(
                DiagnosticCode::ParamOutOfRange,
                value.origin().location(),
                path,
                &what,
            );
        }
        if let Some(max) = node.max_items
            && length > max
        {
            let what = format!("must have at most {max} items, found {length}");
            self.add(
                DiagnosticCode::ParamOutOfRange,
                value.origin().location(),
                path,
                &what,
            );
        }
        let Some(schema) = node.items else { return };
        for (i, item) in items.iter().enumerate() {
            let last = path.pop().unwrap_or_default();
            path.push(format!("{last}[{i}]"));
            self.node(schema, item, path);
            path.pop();
            path.push(last);
        }
    }
}

fn subject(path: &[String]) -> String {
    if path.is_empty() {
        "value ".to_string()
    } else {
        format!("parameter `{}` ", path.join("."))
    }
}

fn dotted(path: &[String], name: &str) -> String {
    if path.is_empty() {
        name.to_string()
    } else {
        format!("{}.{name}", path.join("."))
    }
}

/// A bound, the orderings that violate it, and its operator for messages.
type Rule = (Option<f64>, fn(Ordering) -> bool, &'static str);

/// Orders a number against a bound; integers are compared exactly against integral bounds.
fn compare(value: &LKind, bound: f64) -> Option<Ordering> {
    match value {
        LKind::Integer(i) => match exact_i64(bound) {
            Some(b) => Some(i.cmp(&b)),
            #[expect(
                clippy::cast_precision_loss,
                reason = "non-integral bounds are f64 anyway"
            )]
            None => (*i as f64).partial_cmp(&bound),
        },
        LKind::Float(f) => f.partial_cmp(&bound),
        _ => None,
    }
}

/// `bound` as an `i64` when it is a whole number in range.
fn exact_i64(bound: f64) -> Option<i64> {
    // 2^63 as f64 is exactly representable; the range check keeps the cast exact.
    let limit = 9_223_372_036_854_775_808.0;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "whole and within the i64 range"
    )]
    (bound.fract() == 0.0 && (-limit..limit).contains(&bound)).then_some(bound as i64)
}

/// JSON equality where numbers compare by value (`1` equals `1.0`).
fn json_eq(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(l), Value::Number(r)) => match (l.as_i64(), r.as_i64()) {
            (Some(li), Some(ri)) => li == ri,
            _ => l.as_f64() == r.as_f64(),
        },
        (Value::Array(l), Value::Array(r)) => {
            l.len() == r.len() && l.iter().zip(r).all(|(a, b)| json_eq(a, b))
        }
        (Value::Object(l), Value::Object(r)) => {
            l.len() == r.len()
                && l.iter()
                    .all(|(key, a)| r.get(key).is_some_and(|b| json_eq(a, b)))
        }
        _ => left == right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile(json: &str) -> Result<CompiledSchema, SchemaError> {
        let schema = ParamSchema::from_json_str(json).map_err(|e| error("", e.to_string()))?;
        CompiledSchema::compile(&schema)
    }

    #[test]
    fn unsupported_keyword_pointer_names_the_keyword() {
        let e = compile(r#"{"properties":{"a/b":{"pattern":"x"}}}"#).err();

        assert_eq!(
            e.map(|e| e.pointer),
            Some("/properties/a~1b/pattern".to_string())
        );
    }

    #[test]
    fn a_cycle_through_properties_is_rejected() {
        let json = r##"{"$defs":{"t":{"type":"object","properties":{"c":{"$ref":"#/$defs/t"}}}},"$ref":"#/$defs/t"}"##;

        assert!(compile(json).is_err());
    }

    #[test]
    fn bad_keyword_arguments_are_rejected() {
        for json in [
            r#"{"minLength":-1}"#,
            r#"{"minimum":"0"}"#,
            r#"{"required":[1]}"#,
            r#"{"enum":[]}"#,
            r#"{"oneOf":[]}"#,
            r#"{"properties":{"a":true}}"#,
            r#"{"items":[]}"#,
            r#"{"additionalProperties":"no"}"#,
            r#"{"type":[]}"#,
            r#"{"properties":{"a":{"$defs":{}}}}"#,
        ] {
            assert!(compile(json).is_err(), "{json}");
        }
    }

    #[test]
    fn numbers_in_enum_compare_by_value() {
        assert!(json_eq(&serde_json::json!(1), &serde_json::json!(1.0)));
        assert!(!json_eq(&serde_json::json!(1), &serde_json::json!(2.0)));
    }
}
