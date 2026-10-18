//! Template sets: duplicates, parents, cycles, inheritance merge, component checks (ADR 0010,
//! REQ-T-1..4, ASM-054 rule 3, ASM-056).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use cgf_core::{ComponentId, ComponentKind, ParamValue, Provenance, TemplateId};

use crate::components::syntax_note;
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::graph_read::is_graph_path;
use crate::graph_resolve::GraphSet;
use crate::loader::Loader;
use crate::overlay::Overlay;
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
    /// The behavior and its merged parameters, if the chain names one (ADR 0010 merge rules).
    pub behavior: Option<ComponentRef>,
    /// The sensors, in template order (a child's list replaces its parent's).
    pub sensors: Vec<ComponentRef>,
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
    /// depend on the order they are given in (REQ-K-4). `unread` lists template paths that could
    /// not be read; they only add a note to `unknown_template` errors.
    ///
    /// A template that cannot be fully resolved (its chain reaches a broken, duplicate, missing or
    /// cyclic template) is still checked for what it states itself (ASM-056).
    pub(crate) fn load_templates(
        &self,
        files: &[SourceFile],
        unread: &[PathBuf],
    ) -> LoadedTemplates {
        // ASM-074: `*.behavior.toml` sources are graphs, read by the graph reader.
        let (graph_files, mut sorted): (Vec<&SourceFile>, Vec<&SourceFile>) =
            files.iter().partition(|f| is_graph_path(f.path()));
        sorted.sort_by(|a, b| a.path().cmp(b.path()));
        let graphs = self.load_graphs(&graph_files, Provenance::File);
        // Templates resolve on a loader that also knows this load's graphs (WI-14).
        let overlay = self.overlay(&graphs);
        let mut set = SetBuilder::default();
        set.diagnostics.extend(graphs.diagnostics.iter().cloned());
        for file in sorted {
            set.add_file(file);
        }
        set.check_set(unread);
        let mut templates = BTreeMap::new();
        let mut unusable = set.bad.clone();
        for (index, def) in set.defs.iter().enumerate() {
            let Some(template) = &def.template else {
                continue;
            };
            let mut local = Vec::new();
            let chain = (set.unique.get(&def.id) == Some(&index))
                .then(|| set.chain_of(&def.id))
                .flatten();
            match chain {
                Some(chain) => {
                    if let Some(resolved) = overlay.resolve_one(&chain, &mut local) {
                        templates.insert(def.id.clone(), resolved);
                    }
                }
                None => {
                    overlay.check_partial(template, &set.known_ancestors(template), &mut local);
                }
            }
            set.diagnostics.extend(local);
        }
        unusable.extend(set.defs.iter().map(|d| d.id.clone()));
        unusable.retain(|id| !templates.contains_key(id));
        LoadedTemplates {
            set: TemplateSet { templates },
            graphs: graphs.set.clone(),
            diagnostics: set.diagnostics,
            unusable,
            unreadable: set.unreadable,
        }
    }
}

impl Overlay<'_> {
    /// What a template says itself, when its chain cannot be resolved: its own component IDs,
    /// its own `[[sensors]]` list (which replaces the parent's), and parameters of a model that
    /// differs from the nearest known ancestor's (the parent's parameters are dropped then).
    fn check_partial(
        &self,
        template: &RawTemplate,
        ancestors: &[&RawTemplate],
        out: &mut Vec<Diagnostic>,
    ) {
        let section = |pick: fn(&RawTemplate) -> Option<&RawComponent>| {
            ancestors
                .iter()
                .find_map(|a| pick(a).and_then(|c| c.id.as_ref()))
                .map(|(id, _)| id.clone())
        };
        let known = [
            (
                ComponentKind::MotionModel,
                template.motion.as_ref(),
                section(|t| t.motion.as_ref()),
            ),
            (
                ComponentKind::Behavior,
                template.behavior.as_ref(),
                section(|t| t.behavior.as_ref()),
            ),
        ];
        for (kind, own, ancestor) in known {
            let Some(own) = own else { continue };
            match &own.id {
                Some((id, at)) if ancestor.as_ref().is_some_and(|a| a != id) => {
                    let params = own.params.clone().unwrap_or_else(|| {
                        LTable::from_entries(own.origin.clone(), BTreeMap::new())
                    });
                    let _ = self.check(kind, id, at, &params, out);
                }
                Some((id, at)) => {
                    if self.loader.check_registered(kind, id, at, out) {
                        self.check_own(kind, id, own.params.as_ref(), out);
                    }
                }
                // The model comes from a readable ancestor: check what the child writes.
                None => {
                    if let Some(id) = &ancestor {
                        self.check_own(kind, id, own.params.as_ref(), out);
                    }
                }
            }
        }
        if let Some(sensors) = &template.sensors {
            self.check_sensor_list(sensors, out);
        }
    }

    fn resolve_one(
        &self,
        chain: &[&RawTemplate],
        out: &mut Vec<Diagnostic>,
    ) -> Option<ResolvedTemplate> {
        let head = chain.first()?;
        let motion = self.resolve_motion(head, chain, out);
        let behavior = self.check_behavior(chain, out);
        let sensors = self.check_sensors(chain, out);
        let (id, params, located) = motion?;
        Some(ResolvedTemplate {
            id: head.id.0.clone(),
            chain: chain.iter().map(|t| t.id.0.clone()).collect(),
            description: head.description.clone(),
            motion: ComponentRef { id, params },
            network: merge_network(chain),
            source: head.source.clone(),
            behavior: behavior.ok()?,
            sensors: sensors?,
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
        let json = self.check(ComponentKind::MotionModel, &id, &id_origin, &params, out)?;
        Some((id.clone(), json, LocatedMotion { id_origin, params }))
    }

    /// The merged `[behavior]`: `Ok(None)` when the chain has none, `Err` when it has errors. The
    /// ID is a registered behavior or a graph of the load (ASM-083).
    fn check_behavior(
        &self,
        chain: &[&RawTemplate],
        out: &mut Vec<Diagnostic>,
    ) -> Result<Option<ComponentRef>, Failed> {
        let Some(merged) = merge_component(chain, |t| t.behavior.as_ref()) else {
            return Ok(None);
        };
        let Some((id, at)) = merged.id.clone() else {
            let message = "`[behavior]` has no `ref`".to_string();
            out.push(Diagnostic::new(
                DiagnosticCode::MissingKey,
                message,
                Some(merged.fallback.location()),
            ));
            return Err(Failed);
        };
        let params = merged.table();
        let json = self
            .check(ComponentKind::Behavior, &id, &at, &params, out)
            .ok_or(Failed)?;
        Ok(Some(ComponentRef { id, params: json }))
    }

    /// Sensors replace as a whole: the most derived `[[sensors]]` list wins.
    fn check_sensors(
        &self,
        chain: &[&RawTemplate],
        out: &mut Vec<Diagnostic>,
    ) -> Option<Vec<ComponentRef>> {
        match chain.iter().find_map(|t| t.sensors.as_ref()) {
            Some(sensors) => self.check_sensor_list(sensors, out),
            None => Some(Vec::new()),
        }
    }

    /// Every sensor is checked; `None` when one has errors.
    fn check_sensor_list(
        &self,
        sensors: &[RawComponent],
        out: &mut Vec<Diagnostic>,
    ) -> Option<Vec<ComponentRef>> {
        let mut checked = Some(Vec::new());
        for sensor in sensors {
            let Some((id, at)) = &sensor.id else { continue };
            let params = sensor
                .params
                .clone()
                .unwrap_or_else(|| LTable::from_entries(sensor.origin.clone(), BTreeMap::new()));
            let json = self.check(ComponentKind::Sensor, id, at, &params, out);
            checked = match (checked, json) {
                (Some(mut list), Some(params)) => {
                    list.push(ComponentRef {
                        id: id.clone(),
                        params,
                    });
                    Some(list)
                }
                _ => None,
            };
        }
        checked
    }
}

/// A section that has errors (reported already).
struct Failed;

/// The outcome of loading a set of template files.
pub(crate) struct LoadedTemplates {
    /// The templates that resolved cleanly.
    pub(crate) set: TemplateSet,
    /// The behavior graphs of the load (ASM-107).
    pub(crate) graphs: GraphSet,
    /// Every diagnostic, unsorted.
    pub(crate) diagnostics: Vec<Diagnostic>,
    /// IDs that exist (or may exist) but could not be used; references to them are not reported
    /// again (ASM-056).
    pub(crate) unusable: BTreeSet<TemplateId>,
    /// Template files with syntax errors whose `id` could not be recovered (ASM-054 rule 3).
    pub(crate) unreadable: Vec<PathBuf>,
}

/// The notes of an `unknown_template` error: files that may define the name but could not be
/// used (ASM-054 rule 3).
pub(crate) fn missing_template_notes(syntax: &[PathBuf], unread: &[PathBuf]) -> Vec<String> {
    let syntax = syntax.iter().map(|p| syntax_note(p));
    let unread = unread.iter().map(|p| {
        format!(
            "may be defined under {}, which could not be read",
            p.display()
        )
    });
    syntax.chain(unread).collect()
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
            if state.id.as_ref().is_some_and(|(old, _)| old != id) {
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

/// One definition of a template ID found in a file; broken files keep their ID when it can be
/// recovered.
struct Def {
    id: TemplateId,
    at: Origin,
    template: Option<RawTemplate>,
}

/// Files read so far and the set-level facts about them.
#[derive(Default)]
struct SetBuilder {
    diagnostics: Vec<Diagnostic>,
    /// Every definition, in path order.
    defs: Vec<Def>,
    /// IDs whose template cannot be used; dependents are not resolved (ASM-056).
    bad: BTreeSet<TemplateId>,
    /// IDs defined exactly once in a clean file: ID to index in `defs`.
    unique: BTreeMap<TemplateId, usize>,
    /// Files with syntax errors whose `id` could not be recovered (ASM-054 rule 3).
    unreadable: Vec<PathBuf>,
}

impl SetBuilder {
    fn add_file(&mut self, file: &SourceFile) {
        let read = read_template(file);
        self.diagnostics.extend(read.diagnostics);
        match (read.template, read.known_id, read.known_id_origin) {
            (Some(template), _, _) => self.defs.push(Def {
                id: template.id.0.clone(),
                at: template.id.1.clone(),
                template: Some(template),
            }),
            (None, Some(id), Some(at)) => self.defs.push(Def {
                id,
                at,
                template: None,
            }),
            (None, _, _) if read.syntax_errors => self.unreadable.push(file.path().to_path_buf()),
            (None, _, _) => {}
        }
    }

    /// Duplicates, cycles and unknown parents; afterwards every `bad` ID is out of the graph.
    fn check_set(&mut self, unread: &[PathBuf]) {
        self.check_duplicates();
        self.unique = self
            .defs
            .iter()
            .enumerate()
            .filter(|(_, d)| d.template.is_some() && !self.bad.contains(&d.id))
            .map(|(i, d)| (d.id.clone(), i))
            .collect();
        self.check_cycles();
        self.check_parents(unread);
    }

    fn template(&self, id: &TemplateId) -> Option<&RawTemplate> {
        self.unique
            .get(id)
            .and_then(|&i| self.defs.get(i))
            .and_then(|d| d.template.as_ref())
    }

    /// Every definition after the first of an ID is `duplicate_id` (note at the first), whether
    /// the definitions are in clean files or in files with errors; a broken definition makes the
    /// ID unusable.
    fn check_duplicates(&mut self) {
        let mut first: BTreeMap<TemplateId, Origin> = BTreeMap::new();
        for def in &self.defs {
            if def.template.is_none() {
                self.bad.insert(def.id.clone());
            }
            match first.get(&def.id) {
                None => {
                    first.insert(def.id.clone(), def.at.clone());
                }
                Some(origin) => {
                    let message = format!("template `{}` is defined more than once", def.id);
                    self.diagnostics.push(
                        Diagnostic::new(
                            DiagnosticCode::DuplicateId,
                            message,
                            Some(def.at.location()),
                        )
                        .with_note("first defined here", Some(origin.location())),
                    );
                    self.bad.insert(def.id.clone());
                }
            }
        }
    }

    fn parent_of(&self, id: &TemplateId) -> Option<&TemplateId> {
        let parent = &self.template(id)?.extends.as_ref()?.0;
        self.unique.contains_key(parent).then_some(parent)
    }

    /// One diagnostic per cycle, at the `extends` of its smallest ID.
    fn check_cycles(&mut self) {
        let mut done: BTreeSet<TemplateId> = BTreeSet::new();
        let ids: Vec<TemplateId> = self.unique.keys().cloned().collect();
        for start in ids {
            let mut path: Vec<TemplateId> = Vec::new();
            let mut current = Some(start);
            while let Some(id) = current {
                if done.contains(&id) {
                    break;
                }
                if let Some(at) = path.iter().position(|p| *p == id) {
                    let members = path.split_off(at);
                    done.extend(members.iter().cloned());
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
        let edge = |id: &TemplateId| self.template(id).and_then(|t| t.extends.as_ref());
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
    fn check_parents(&mut self, unread: &[PathBuf]) {
        let mut missing = Vec::new();
        for (index, def) in self.defs.iter().enumerate() {
            let Some((parent, at)) = def.template.as_ref().and_then(|t| t.extends.as_ref()) else {
                continue;
            };
            if !self.unique.contains_key(parent) && !self.bad.contains(parent) {
                let unique = self.unique.get(&def.id) == Some(&index);
                missing.push((def.id.clone(), parent.clone(), at.clone(), unique));
            }
        }
        for (id, parent, at, unique) in missing {
            let message = format!("template `{parent}` (extends) is not defined");
            let mut diagnostic = Diagnostic::new(
                DiagnosticCode::UnknownTemplate,
                message,
                Some(at.location()),
            );
            for note in missing_template_notes(&self.unreadable, unread) {
                diagnostic = diagnostic.with_note(note, None);
            }
            self.diagnostics.push(diagnostic);
            if unique {
                self.bad.insert(id);
            }
        }
    }

    /// The templates a template extends that can be read, nearest first (stops at a missing,
    /// broken or duplicate one, or a loop).
    fn known_ancestors(&self, template: &RawTemplate) -> Vec<&RawTemplate> {
        let mut out: Vec<&RawTemplate> = Vec::new();
        let mut next = template.extends.as_ref().map(|(id, _)| id);
        while let Some(id) = next {
            let Some(parent) = self.template(id) else {
                break;
            };
            if parent.id.0 == template.id.0 || out.iter().any(|t| t.id.0 == parent.id.0) {
                break;
            }
            out.push(parent);
            next = parent.extends.as_ref().map(|(id, _)| id);
        }
        out
    }

    /// The chain `[self, parent, .., root]`, or `None` when it reaches an unusable template.
    fn chain_of(&self, id: &TemplateId) -> Option<Vec<&RawTemplate>> {
        let mut chain = Vec::new();
        let mut current = id;
        loop {
            if self.bad.contains(current) || chain.len() > self.defs.len() {
                return None;
            }
            let template = self.template(current)?;
            chain.push(template);
            match &template.extends {
                None => return Some(chain),
                Some((parent, _)) => current = parent,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use cgf_core::Registry;

    use super::*;

    fn text(id: &str, extends: &str) -> SourceFile {
        let body = format!("schema_version = 1\nid = \"{id}\"\nextends = \"{extends}\"\n");
        SourceFile::new(format!("{id}.toml"), body)
    }

    /// Item 3: a cycle is reported once by the builder itself, not once per member and then
    /// hidden by the de-duplication.
    #[test]
    fn a_three_template_cycle_is_one_raw_diagnostic() {
        let registry = Registry::new();
        let Ok(loader) = Loader::new(&registry) else {
            panic!("loader");
        };
        let files = [
            text("a.one", "b.two"),
            text("b.two", "c.three"),
            text("c.three", "a.one"),
        ];

        let result = loader.load_templates(&files, &[]);

        let cycles = result
            .diagnostics
            .iter()
            .filter(|d| d.code == DiagnosticCode::InheritanceCycle)
            .count();
        assert_eq!(cycles, 1);
    }

    /// Item 6: a root `[motion]` with params and no model keeps them when a child names a model.
    #[test]
    fn params_written_before_any_model_survive_the_first_model() {
        let root = "schema_version = 1\nid = \"r.r\"\n[motion]\nparams = { a = 3.0 }\n";
        let leaf =
            "schema_version = 1\nid = \"l.l\"\nextends = \"r.r\"\n[motion]\nmodel = \"m.m\"\n";
        let root = read_template(&SourceFile::new("r.toml", root)).template;
        let leaf = read_template(&SourceFile::new("l.toml", leaf)).template;
        let (Some(root), Some(leaf)) = (root, leaf) else {
            panic!("templates read");
        };

        let merged = merge_component(&[&leaf, &root], |t| t.motion.as_ref());

        let params = merged.map(|m| m.table().to_param());
        assert_eq!(params, Some(serde_json::json!({ "a": 3.0 })));
    }
}
