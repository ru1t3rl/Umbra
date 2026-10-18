//! File discovery and the path-based entry points (ASM-053, ASM-067, REQ-T-6 groundwork).
//!
//! Everything else works on in-memory [`SourceFile`]s; this module is the thin I/O layer.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::diagnostic::{Diagnostic, DiagnosticCode, Diagnostics, sort_and_dedup};
use crate::graph_read::is_graph_path;
use crate::loader::{Loader, ValidationReport};
use crate::resolve::LoadedTemplates;
use crate::scenario::Scenario;
use crate::scenario_read::{is_scenario, template_paths};
use crate::source::SourceFile;

/// Directories nested deeper than this are not searched (a guard, reported as a diagnostic).
const MAX_DEPTH: usize = 64;

/// The files and diagnostics of a scenario load.
pub(crate) struct ScenarioLoad {
    pub(crate) scenario: Option<Scenario>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    /// Every file read, in sorted path order.
    pub(crate) files: Vec<PathBuf>,
}

/// The template files a scenario's `template_paths` lead to.
struct Collected {
    templates: Vec<SourceFile>,
    /// Paths (files or directories) that could not be read.
    unread: Vec<PathBuf>,
    /// Template and scenario files found, sorted.
    files: Vec<PathBuf>,
}

/// An `io` diagnostic for `path`.
pub(crate) fn io_diagnostic(path: &Path, error: &io::Error) -> Diagnostic {
    let message = format!("cannot read {}: {error}", path.display());
    Diagnostic::new(DiagnosticCode::Io, message, None)
}

/// Every `*.toml` file (lower-case extension) under `dir`, recursively, in sorted path order. A
/// symlink to a regular file counts; directory symlinks are not followed (ASM-053). Unreadable
/// directories become `io` diagnostics.
pub(crate) fn toml_files(dir: &Path, diagnostics: &mut Vec<Diagnostic>) -> Vec<PathBuf> {
    let mut found = Vec::new();
    walk(dir, 0, &mut found, diagnostics);
    found.sort();
    found
}

/// The `*.toml` files directly in `dir` (not below it), sorted: the context of a single template
/// file (ASM-067).
fn sibling_files(dir: &Path, diagnostics: &mut Vec<Diagnostic>) -> Vec<PathBuf> {
    let mut found = Vec::new();
    match fs::read_dir(dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let path = entry.path();
                let is_file = entry.file_type().is_ok_and(|k| k.is_file())
                    || (entry.file_type().is_ok_and(|k| k.is_symlink())
                        && fs::metadata(&path).is_ok_and(|m| m.is_file()));
                if is_file && is_toml(&path) {
                    found.push(path);
                }
            }
        }
        Err(error) => diagnostics.push(io_diagnostic(dir, &error)),
    }
    found.sort();
    found
}

fn is_toml(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "toml")
}

fn walk(dir: &Path, depth: usize, found: &mut Vec<PathBuf>, diagnostics: &mut Vec<Diagnostic>) {
    if depth > MAX_DEPTH {
        let message = format!(
            "{} is nested deeper than {MAX_DEPTH} directories and was not searched",
            dir.display()
        );
        diagnostics.push(Diagnostic::new(DiagnosticCode::Io, message, None));
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            diagnostics.push(io_diagnostic(dir, &error));
            return;
        }
    };
    for entry in entries {
        match entry {
            Ok(entry) => visit(&entry, depth, found, diagnostics),
            Err(error) => diagnostics.push(io_diagnostic(dir, &error)),
        }
    }
}

fn visit(
    entry: &fs::DirEntry,
    depth: usize,
    found: &mut Vec<PathBuf>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let path = entry.path();
    match entry.file_type() {
        Ok(kind) if kind.is_dir() => walk(&path, depth + 1, found, diagnostics),
        Ok(kind) if kind.is_file() && is_toml(&path) => found.push(path),
        Ok(kind) if kind.is_symlink() && is_toml(&path) => {
            // Only a link to a regular file counts; a link to a directory is never followed.
            if fs::metadata(&path).is_ok_and(|m| m.is_file()) {
                found.push(path);
            }
        }
        Ok(_) => {}
        Err(error) => diagnostics.push(io_diagnostic(&path, &error)),
    }
}

/// Files read so far by one validation, by identity: each file is read once, and a failure is
/// remembered with its diagnostic.
type SourceCache = BTreeMap<PathBuf, Result<SourceFile, Diagnostic>>;

/// Reads `path` through the cache.
fn cached(cache: &mut SourceCache, path: &Path) -> Result<SourceFile, Diagnostic> {
    cache
        .entry(identity(path))
        .or_insert_with(|| read_source(path))
        .clone()
}

/// Reads `paths` into sources; unreadable files become `io` diagnostics and are listed in `unread`.
fn read_all(
    paths: &[PathBuf],
    diagnostics: &mut Vec<Diagnostic>,
    unread: &mut Vec<PathBuf>,
) -> Vec<SourceFile> {
    let mut out = Vec::new();
    for path in paths {
        match fs::read_to_string(path) {
            Ok(text) => out.push(SourceFile::new(path.clone(), text)),
            Err(error) => {
                diagnostics.push(io_diagnostic(path, &error));
                unread.push(path.clone());
            }
        }
    }
    out
}

/// A key that is the same for two spellings of one file.
fn identity(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

impl Loader<'_> {
    /// Loads a scenario file and the templates under its `template_paths` (relative to the
    /// scenario file).
    ///
    /// # Errors
    /// Every diagnostic, sorted; a file that is not a scenario is `not_a_scenario`.
    pub fn load_scenario(&self, path: &Path) -> Result<Scenario, Diagnostics> {
        let load = match read_source(path) {
            Ok(file) => self.scenario_pipeline(&file),
            Err(diagnostic) => ScenarioLoad {
                scenario: None,
                diagnostics: vec![diagnostic],
                files: Vec::new(),
            },
        };
        match (load.scenario, Diagnostics::new(load.diagnostics)) {
            (_, Some(errors)) => Err(errors),
            (Some(scenario), None) => Ok(scenario),
            (None, None) => Err(Diagnostics::from_one(Diagnostic::new(
                DiagnosticCode::NotAScenario,
                format!("{} could not be loaded as a scenario", path.display()),
                None,
            ))),
        }
    }

    /// The pipeline shared by `load_scenario` and `validate_path` (ASM-058).
    pub(crate) fn scenario_pipeline(&self, file: &SourceFile) -> ScenarioLoad {
        let path = file.path();
        let mut diagnostics = Vec::new();
        let is_graph = is_graph_path(path);
        if is_graph || (!is_scenario(file) && !has_syntax_errors(file)) {
            let message = if is_graph {
                format!(
                    "{} is a behavior graph, not a scenario (ASM-074)",
                    path.display()
                )
            } else {
                format!(
                    "{} is not a scenario (it has no `[scenario]` table)",
                    path.display()
                )
            };
            diagnostics.push(Diagnostic::new(DiagnosticCode::NotAScenario, message, None));
            return ScenarioLoad {
                scenario: None,
                diagnostics,
                files: vec![path.to_path_buf()],
            };
        }
        let collected = collect_templates(file, &mut diagnostics, &mut SourceCache::new());
        let (scenario, own) = self.build_scenario(file, &collected.templates, &collected.unread);
        diagnostics.extend(own);
        let mut files = collected.files;
        files.push(path.to_path_buf());
        files.sort();
        files.dedup();
        ScenarioLoad {
            scenario,
            diagnostics,
            files,
        }
    }

    /// Checks a scenario file, a template file or a directory (recursively). Never runs anything:
    /// components are only dry-run through `Factory::build`.
    pub fn validate_path(&self, path: &Path) -> ValidationReport {
        let mut diagnostics = Vec::new();
        let mut files = Vec::new();
        match fs::metadata(path) {
            Err(error) => diagnostics.push(io_diagnostic(path, &error)),
            Ok(meta) if meta.is_dir() => {
                self.validate_directory(path, &mut files, &mut diagnostics);
            }
            Ok(_) => self.validate_file(path, &mut files, &mut diagnostics),
        }
        files.sort();
        files.dedup();
        sort_and_dedup(&mut diagnostics);
        ValidationReport {
            files_checked: files,
            diagnostics,
        }
    }

    fn validate_file(
        &self,
        path: &Path,
        files: &mut Vec<PathBuf>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let file = match read_source(path) {
            Ok(file) => file,
            Err(diagnostic) => {
                diagnostics.push(diagnostic);
                return;
            }
        };
        if is_graph_path(path) {
            // ASM-074: a graph file is checked with its direct siblings, like a template.
            files.push(path.to_path_buf());
            diagnostics.extend(self.validate_template_file(&file));
        } else if is_scenario(&file) {
            let load = self.scenario_pipeline(&file);
            files.extend(load.files);
            diagnostics.extend(load.diagnostics);
        } else {
            files.push(path.to_path_buf());
            diagnostics.extend(self.validate_template_file(&file));
        }
    }

    /// A single template or graph file is checked in the context of its directory (its parents
    /// and the graphs it calls are usually next to it); only diagnostics located in the file, or
    /// whose notes point into it (a duplicate or a cycle that involves it), are reported
    /// (ASM-067, ASM-074).
    fn validate_template_file(&self, file: &SourceFile) -> Vec<Diagnostic> {
        let mut ignored = Vec::new();
        let mut unread = Vec::new();
        let dir = file.path().parent().unwrap_or_else(|| Path::new(""));
        let siblings = sibling_files(dir, &mut ignored);
        let me = identity(file.path());
        let mut sources: Vec<SourceFile> = read_all(&siblings, &mut ignored, &mut unread)
            .into_iter()
            .filter(|s| identity(s.path()) != me && !is_scenario_source(s))
            .collect();
        sources.push(file.clone());
        let loaded = self.load_templates(&sources, &unread);
        loaded
            .diagnostics
            .into_iter()
            .filter(|d| involves(d, &me))
            .collect()
    }

    /// The directory's own template files are validated as one set; each scenario is resolved
    /// against exactly the templates `load_scenario` would build for it (its own `template_paths`).
    /// Files are read once and a template set is loaded (and its components built) once, however
    /// many scenarios or the directory itself use it.
    fn validate_directory(
        &self,
        dir: &Path,
        files: &mut Vec<PathBuf>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let paths = toml_files(dir, diagnostics);
        files.extend(paths.iter().cloned());
        let mut cache = SourceCache::new();
        let mut sources = Vec::new();
        let mut unread = Vec::new();
        for path in &paths {
            match cached(&mut cache, path) {
                Ok(source) => sources.push(source),
                Err(diagnostic) => {
                    diagnostics.push(diagnostic);
                    unread.push(path.clone());
                }
            }
        }
        let (scenarios, templates): (Vec<_>, Vec<_>) =
            sources.into_iter().partition(is_scenario_source);
        let mut sets: BTreeMap<BTreeSet<PathBuf>, LoadedTemplates> = BTreeMap::new();
        self.load_set(&templates, &unread, &mut sets, diagnostics);
        for scenario in &scenarios {
            let collected = collect_templates(scenario, diagnostics, &mut cache);
            files.extend(collected.files);
            let loaded = self.load_set(
                &collected.templates,
                &collected.unread,
                &mut sets,
                diagnostics,
            );
            diagnostics.extend(self.read_scenario(scenario, loaded, &collected.unread).1);
        }
    }

    /// The loaded set for these template sources, loading it (and recording its diagnostics)
    /// only the first time that exact set is seen.
    fn load_set<'s>(
        &self,
        templates: &[SourceFile],
        unread: &[PathBuf],
        sets: &'s mut BTreeMap<BTreeSet<PathBuf>, LoadedTemplates>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> &'s LoadedTemplates {
        let key: BTreeSet<PathBuf> = templates.iter().map(|t| identity(t.path())).collect();
        sets.entry(key).or_insert_with(|| {
            let loaded = self.load_templates(templates, unread);
            diagnostics.extend(loaded.diagnostics.iter().cloned());
            loaded
        })
    }
}

/// Whether a diagnostic is located in the file or has a note that points into it.
fn involves(diagnostic: &Diagnostic, file: &Path) -> bool {
    let here = |l: &crate::diagnostic::Location| identity(&l.path) == file;
    diagnostic.location.as_ref().is_some_and(here)
        || diagnostic
            .notes
            .iter()
            .any(|n| n.location.as_ref().is_some_and(here))
}

fn read_source(path: &Path) -> Result<SourceFile, Diagnostic> {
    fs::read_to_string(path)
        .map(|text| SourceFile::new(path, text))
        .map_err(|error| io_diagnostic(path, &error))
}

/// Reads the template files under the scenario's `template_paths` (recursively, scenario files
/// skipped), once per file even when reached through two spellings of its path.
fn collect_templates(
    scenario: &SourceFile,
    diagnostics: &mut Vec<Diagnostic>,
    cache: &mut SourceCache,
) -> Collected {
    let base = scenario.path().parent().unwrap_or_else(|| Path::new(""));
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut unread = Vec::new();
    for (entry, origin) in template_paths(scenario) {
        let dir = base.join(&entry);
        let mut local = Vec::new();
        let found = toml_files(&dir, &mut local);
        if !local.is_empty() {
            unread.push(dir);
            for mut d in local {
                d.location = Some(origin.location());
                diagnostics.push(d);
            }
        }
        paths.extend(found);
    }
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    seen.insert(identity(scenario.path()));
    paths.retain(|p| seen.insert(identity(p)));
    paths.sort();
    let mut sources = Vec::new();
    for path in &paths {
        match cached(cache, path) {
            Ok(source) => sources.push(source),
            Err(diagnostic) => {
                diagnostics.push(diagnostic);
                unread.push(path.clone());
            }
        }
    }
    let (scenarios, templates): (Vec<_>, Vec<_>) =
        sources.into_iter().partition(is_scenario_source);
    let mut files: Vec<PathBuf> = templates
        .iter()
        .chain(&scenarios)
        .map(|f| f.path().to_path_buf())
        .collect();
    files.sort();
    Collected {
        templates,
        unread,
        files,
    }
}

/// A scenario file: has a `[scenario]` table and is not a behavior graph (ASM-074: the
/// `*.behavior.toml` suffix decides first).
fn is_scenario_source(file: &SourceFile) -> bool {
    !is_graph_path(file.path()) && is_scenario(file)
}

fn has_syntax_errors(file: &SourceFile) -> bool {
    crate::value::parse(file)
        .1
        .iter()
        .any(|d| d.code == DiagnosticCode::Syntax)
}
