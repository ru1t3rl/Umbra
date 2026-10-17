//! Template sets: duplicates, parents, cycles, inheritance merge, component checks (ADR 0010,
//! REQ-T-1..4, ASM-054 rule 3, ASM-056).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use cgf_core::{ComponentId, ComponentKind, ParamValue, TemplateId};

use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::loader::Loader;
use crate::source::SourceFile;
use crate::template::{RawComponent, RawTemplate, read_template};
use crate::value::{LEntry, LTable, Origin};

/// A component by ID with its parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentRef {
    /// The component's ID.
    pub id: ComponentId,
    /// Its parameters.
    pub params: ParamValue,
}

/// A template with inheritance applied.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTemplate {
    /// The template's ID.
    pub id: TemplateId,
    /// The inheritance chain, the template first and the root last.
    pub chain: Vec<TemplateId>,
    /// The description, not inherited.
    pub description: Option<String>,
    /// The motion model and its merged parameters.
    pub motion: ComponentRef,
    /// Opaque `network.<adapter>` tables (REQ-T-4).
    pub network: BTreeMap<String, ParamValue>,
    /// The file the template was defined in.
    pub source: PathBuf,
    /// The merged motion parameters with the origin of every value (for scenario overrides).
    pub(crate) located: LocatedMotion,
}

/// The merged motion parameters of a template, located.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LocatedMotion {
    /// Where the winning `model` value is written.
    pub(crate) id_origin: Origin,
    /// The merged parameters; each value keeps the origin of the file that wrote it.
    pub(crate) params: LTable,
}

/// All templates of a load, by ID.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TemplateSet {
    templates: BTreeMap<TemplateId, ResolvedTemplate>,
}

impl TemplateSet {
    /// The template with this ID.
    pub fn get(&self, id: &TemplateId) -> Option<&ResolvedTemplate> {
        self.templates.get(id)
    }

    /// All templates in ID order.
    pub fn iter(&self) -> impl Iterator<Item = &ResolvedTemplate> {
        self.templates.values()
    }

    /// How many templates there are.
    pub fn len(&self) -> usize {
        self.templates.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }
}

impl Loader<'_> {
    /// Reads, checks and resolves template files. Returns the templates that resolved cleanly and
    /// every diagnostic, unsorted. Files are processed in path order, so the result does not
    /// depend on the order they are given in (REQ-K-4).
    pub(crate) fn load_templates(&self, files: &[SourceFile]) -> (TemplateSet, Vec<Diagnostic>) {
        let mut sorted: Vec<&SourceFile> = files.iter().collect();
        sorted.sort_by(|a, b| a.path().cmp(b.path()));
        let mut set = SetBuilder::default();
        for file in sorted {
            set.add_file(file);
        }
        set.check_set();
        let mut templates = BTreeMap::new();
        for id in set.defs.keys() {
            let Some(chain) = set.chain_of(id) else {
                continue;
            };
            let mut local = Vec::new();
            if let Some(resolved) = self.resolve_one(&chain, &mut local) {
                templates.insert(id.clone(), resolved);
            }
            set.diagnostics.extend(local);
        }
        (TemplateSet { templates }, set.diagnostics)
    }

    fn resolve_one(
        &self,
        chain: &[&RawTemplate],
        out: &mut Vec<Diagnostic>,
    ) -> Option<ResolvedTemplate> {
        let head = chain.first()?;
        let motion = self.resolve_motion(head, chain, out);
        let mut ok = motion.is_some();
        ok &= self.check_behavior(chain, out);
        ok &= self.check_sensors(chain, out);
        let (id, params, located) = motion?;
        ok.then(|| ResolvedTemplate {
            id: head.id.0.clone(),
            chain: chain.iter().map(|t| t.id.0.clone()).collect(),
            description: head.description.clone(),
            motion: ComponentRef { id, params },
            network: merge_network(chain),
            source: head.source.clone(),
            located,
        })
    }

    fn resolve_motion(
        &self,
        head: &RawTemplate,
        chain: &[&RawTemplate],
        out: &mut Vec<Diagnostic>,
    ) -> Option<(ComponentId, ParamValue, LocatedMotion)> {
        let merged = merge_component(chain, |t| t.motion.as_ref());
        let Some((id, id_origin)) = merged.as_ref().and_then(|m| m.id.clone()) else {
            let message = format!(
                "template `{}` has no motion model: set `motion.model` in it or in a template it extends",
                head.id.0
            );
            out.push(Diagnostic::new(
                DiagnosticCode::MissingKey,
                message,
                Some(head.id.1.location()),
            ));
            return None;
        };
        let params = merged?.table();
        let json =
            self.check_component(ComponentKind::MotionModel, &id, &id_origin, &params, out)?;
        Some((id.clone(), json, LocatedMotion { id_origin, params }))
    }

    /// Behaviors have no registry entries in M2 (ASM-055): their IDs are `unknown_id`.
    fn check_behavior(&self, chain: &[&RawTemplate], out: &mut Vec<Diagnostic>) -> bool {
        let Some(merged) = merge_component(chain, |t| t.behavior.as_ref()) else {
            return true;
        };
        let Some((id, at)) = merged.id.clone() else {
            let message = "`[behavior]` has no `ref`".to_string();
            out.push(Diagnostic::new(
                DiagnosticCode::MissingKey,
                message,
                Some(merged.fallback.location()),
            ));
            return false;
        };
        let params = merged.table();
        self.check_component(ComponentKind::Behavior, &id, &at, &params, out)
            .is_some()
    }

    /// Sensors replace as a whole: the most derived `[[sensors]]` list wins.
    fn check_sensors(&self, chain: &[&RawTemplate], out: &mut Vec<Diagnostic>) -> bool {
        let Some(sensors) = chain.iter().find_map(|t| t.sensors.as_ref()) else {
            return true;
        };
        let mut ok = true;
        for sensor in sensors {
            let Some((id, at)) = &sensor.id else { continue };
            let params = sensor
                .params
                .clone()
                .unwrap_or_else(|| LTable::from_entries(sensor.origin.clone(), BTreeMap::new()));
            ok &= self
                .check_component(ComponentKind::Sensor, id, at, &params, out)
                .is_some();
        }
        ok
    }
}

/// The merged `model`/`ref` and `params` of a component section along a chain.
struct Merged {
    id: Option<(ComponentId, Origin)>,
    /// The section that set the ID (or the last section), where an absent params table is located.
    fallback: Origin,
    params: BTreeMap<String, LEntry>,
    params_origin: Option<Origin>,
}

impl Merged {
    fn table(&self) -> LTable {
        let origin = self
            .params_origin
            .clone()
            .unwrap_or_else(|| self.fallback.clone());
        LTable::from_entries(origin, self.params.clone())
    }
}

/// Folds a section root-first (ADR 0010): the ID is replaced, `params` merge key-wise, and naming
/// a different ID drops the parent's `params`. `None` when no template has the section.
fn merge_component<'a>(
    chain: &[&'a RawTemplate],
    pick: impl Fn(&'a RawTemplate) -> Option<&'a RawComponent>,
) -> Option<Merged> {
    let mut merged: Option<Merged> = None;
    for section in chain.iter().rev().filter_map(|t| pick(t)) {
        let state = merged.get_or_insert_with(|| Merged {
            id: None,
            fallback: section.origin.clone(),
            params: BTreeMap::new(),
            params_origin: None,
        });
        if let Some((id, at)) = &section.id {
            if state.id.as_ref().map(|(old, _)| old) != Some(id) {
                state.params.clear();
                state.params_origin = None;
            }
            state.id = Some((id.clone(), at.clone()));
            state.fallback = section.origin.clone();
        }
        if let Some(params) = &section.params {
            for (key, entry) in params.iter() {
                state.params.insert(key.to_string(), entry.clone());
            }
            state.params_origin = Some(params.origin().clone());
        }
    }
    merged
}

/// Each `network.<adapter>` table replaces the parent's table for that adapter (REQ-T-4).
fn merge_network(chain: &[&RawTemplate]) -> BTreeMap<String, ParamValue> {
    let mut out = BTreeMap::new();
    for template in chain.iter().rev() {
        for (adapter, table) in &template.network {
            out.insert(adapter.clone(), table.to_param());
        }
    }
    out
}

/// Files read so far and the set-level facts about them.
#[derive(Default)]
struct SetBuilder {
    diagnostics: Vec<Diagnostic>,
    defs: BTreeMap<TemplateId, RawTemplate>,
    /// IDs whose template cannot be used; dependents get no diagnostic (ASM-056).
    bad: BTreeSet<TemplateId>,
    /// Files with syntax errors whose `id` could not be recovered (ASM-054 rule 3).
    unreadable: Vec<PathBuf>,
}

impl SetBuilder {
    fn add_file(&mut self, file: &SourceFile) {
        let read = read_template(file);
        self.diagnostics.extend(read.diagnostics);
        match (read.template, read.known_id) {
            (Some(template), _) => self.add_template(template),
            (None, Some(id)) => {
                self.bad.insert(id);
            }
            (None, None) if read.syntax_errors => self.unreadable.push(file.path().to_path_buf()),
            (None, None) => {}
        }
    }

    fn add_template(&mut self, template: RawTemplate) {
        let id = template.id.0.clone();
        if let Some(first) = self.defs.get(&id) {
            let message = format!("template `{id}` is defined more than once");
            self.diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::DuplicateId,
                    message,
                    Some(template.id.1.location()),
                )
                .with_note("first defined here", Some(first.id.1.location())),
            );
            self.bad.insert(id);
        } else {
            self.defs.insert(id, template);
        }
    }

    /// Duplicates, cycles and unknown parents; afterwards every `bad` ID is out of the graph.
    fn check_set(&mut self) {
        for id in self.bad.clone() {
            self.defs.remove(&id);
        }
        self.check_cycles();
        self.check_parents();
    }

    fn parent_of(&self, id: &TemplateId) -> Option<&TemplateId> {
        let parent = &self.defs.get(id)?.extends.as_ref()?.0;
        self.defs.contains_key(parent).then_some(parent)
    }

    /// One diagnostic per cycle, at the `extends` of its smallest ID.
    fn check_cycles(&mut self) {
        let mut done: BTreeSet<TemplateId> = BTreeSet::new();
        let ids: Vec<TemplateId> = self.defs.keys().cloned().collect();
        for start in ids {
            let mut path: Vec<TemplateId> = Vec::new();
            let mut current = Some(start);
            while let Some(id) = current {
                if done.contains(&id) {
                    break;
                }
                if let Some(at) = path.iter().position(|p| *p == id) {
                    let members = path.split_off(at);
                    self.report_cycle(members);
                    break;
                }
                current = self.parent_of(&id).cloned();
                path.push(id);
            }
            done.extend(path);
        }
    }

    fn report_cycle(&mut self, mut members: Vec<TemplateId>) {
        let smallest = (0..members.len())
            .min_by_key(|&i| members.get(i).cloned())
            .unwrap_or(0);
        members.rotate_left(smallest);
        let names: Vec<String> = members.iter().map(|m| format!("`{m}`")).collect();
        let message = format!(
            "template inheritance cycle: {} -> {}",
            names.join(" -> "),
            names.first().cloned().unwrap_or_default()
        );
        let edge = |id: &TemplateId| self.defs.get(id).and_then(|t| t.extends.as_ref());
        let at = members
            .first()
            .and_then(edge)
            .map(|(_, origin)| origin.location());
        let mut diagnostic = Diagnostic::new(DiagnosticCode::InheritanceCycle, message, at);
        for member in members.iter().skip(1) {
            if let Some((parent, origin)) = edge(member) {
                let note = format!("`{member}` extends `{parent}`");
                diagnostic = diagnostic.with_note(note, Some(origin.location()));
            }
        }
        self.diagnostics.push(diagnostic);
        self.bad.extend(members);
    }

    /// `extends` naming a template that does not exist. Parents that exist but are unusable
    /// (duplicate, broken, in a cycle) are silent: their error is already reported.
    fn check_parents(&mut self) {
        let mut missing = Vec::new();
        for (id, template) in &self.defs {
            let Some((parent, at)) = &template.extends else {
                continue;
            };
            if !self.defs.contains_key(parent) && !self.bad.contains(parent) {
                missing.push((id.clone(), parent.clone(), at.clone()));
            }
        }
        for (id, parent, at) in missing {
            let message = format!("template `{parent}` (extends) is not defined");
            let mut diagnostic = Diagnostic::new(
                DiagnosticCode::UnknownTemplate,
                message,
                Some(at.location()),
            );
            for path in &self.unreadable {
                let note = format!(
                    "may be defined in {}, which has syntax errors",
                    path.display()
                );
                diagnostic = diagnostic.with_note(note, None);
            }
            self.diagnostics.push(diagnostic);
            self.bad.insert(id);
        }
    }

    /// The chain `[self, parent, .., root]`, or `None` when it reaches an unusable template.
    fn chain_of(&self, id: &TemplateId) -> Option<Vec<&RawTemplate>> {
        let mut chain = Vec::new();
        let mut current = id;
        loop {
            if self.bad.contains(current) || chain.len() > self.defs.len() {
                return None;
            }
            let template = self.defs.get(current)?;
            chain.push(template);
            match &template.extends {
                None => return Some(chain),
                Some((parent, _)) => current = parent,
            }
        }
    }
}
