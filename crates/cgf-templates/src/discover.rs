//! File discovery and the path-based entry points (ASM-053, REQ-T-6 groundwork).
//!
//! Everything else works on in-memory [`SourceFile`]s; this module is the thin I/O layer.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::diagnostic::{Diagnostic, DiagnosticCode, Diagnostics, sort_and_dedup};
use crate::loader::{Loader, ValidationReport};
use crate::scenario::Scenario;
use crate::scenario_read::{is_scenario, template_paths};
use crate::source::SourceFile;

/// Directories nested deeper than this are not searched (a guard, not a feature).
const MAX_DEPTH: usize = 64;

/// The files and diagnostics of a scenario load.
pub(crate) struct ScenarioLoad {
    pub(crate) scenario: Option<Scenario>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    /// Every file read, in sorted path order.
    pub(crate) files: Vec<PathBuf>,
}

/// An `io` diagnostic for `path`.
pub(crate) fn io_diagnostic(path: &Path, error: &io::Error) -> Diagnostic {
    let message = format!("cannot read {}: {error}", path.display());
    Diagnostic::new(DiagnosticCode::Io, message, None)
}

/// Every regular `*.toml` file under `dir`, recursively, in sorted path order. Directory symlinks
/// are not followed (ASM-053). Unreadable directories become `io` diagnostics.
pub(crate) fn toml_files(dir: &Path, diagnostics: &mut Vec<Diagnostic>) -> Vec<PathBuf> {
    let mut found = Vec::new();
    walk(dir, 0, &mut found, diagnostics);
    found.sort();
    found
}

fn walk(dir: &Path, depth: usize, found: &mut Vec<PathBuf>, diagnostics: &mut Vec<Diagnostic>) {
    if depth > MAX_DEPTH {
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
            Ok(entry) => {
                let path = entry.path();
                match entry.file_type() {
                    Ok(kind) if kind.is_dir() => walk(&path, depth + 1, found, diagnostics),
                    Ok(kind) if kind.is_file() && path.extension().is_some_and(|e| e == "toml") => {
                        found.push(path);
                    }
                    Ok(_) => {}
                    Err(error) => diagnostics.push(io_diagnostic(&path, &error)),
                }
            }
            Err(error) => diagnostics.push(io_diagnostic(dir, &error)),
        }
    }
}

/// Reads `paths` into sources; unreadable files become `io` diagnostics.
fn read_all(paths: &[PathBuf], diagnostics: &mut Vec<Diagnostic>) -> Vec<SourceFile> {
    let mut out = Vec::new();
    for path in paths {
        match fs::read_to_string(path) {
            Ok(text) => out.push(SourceFile::new(path.clone(), text)),
            Err(error) => diagnostics.push(io_diagnostic(path, &error)),
        }
    }
    out
}

impl Loader<'_> {
    /// Loads a scenario file and the templates under its `template_paths` (relative to the
    /// scenario file).
    ///
    /// # Errors
    /// Every diagnostic, sorted; a file that is not a scenario is `not_a_scenario`.
    pub fn load_scenario(&self, path: &Path) -> Result<Scenario, Diagnostics> {
        let load = self.scenario_pipeline(path);
        match (load.scenario, Diagnostics::new(load.diagnostics)) {
            (_, Some(errors)) => Err(errors),
            (Some(scenario), None) => Ok(scenario),
            (None, None) => Err(Diagnostics::from_one(io_diagnostic(
                path,
                &io::Error::other("the scenario could not be loaded"),
            ))),
        }
    }

    /// The pipeline shared by `load_scenario` and `validate_path` (ASM-058).
    pub(crate) fn scenario_pipeline(&self, path: &Path) -> ScenarioLoad {
        let mut diagnostics = Vec::new();
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => {
                diagnostics.push(io_diagnostic(path, &error));
                return ScenarioLoad {
                    scenario: None,
                    diagnostics,
                    files: Vec::new(),
                };
            }
        };
        let file = SourceFile::new(path, text);
        if !is_scenario(&file) && !has_syntax_errors(&file) {
            let message = format!(
                "{} is not a scenario (it has no `[scenario]` table)",
                path.display()
            );
            diagnostics.push(Diagnostic::new(DiagnosticCode::NotAScenario, message, None));
            return ScenarioLoad {
                scenario: None,
                diagnostics,
                files: vec![path.to_path_buf()],
            };
        }
        let (templates, template_files, incomplete) = collect_templates(&file, &mut diagnostics);
        let (scenario, own) = self.build_scenario(&file, &templates, incomplete);
        diagnostics.extend(own);
        let mut files = template_files;
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
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => {
                diagnostics.push(io_diagnostic(path, &error));
                return;
            }
        };
        let file = SourceFile::new(path, text);
        if is_scenario(&file) {
            let load = self.scenario_pipeline(path);
            files.extend(load.files);
            diagnostics.extend(load.diagnostics);
        } else {
            files.push(path.to_path_buf());
            diagnostics.extend(self.load_templates(&[file]).diagnostics);
        }
    }

    /// Template files form one set; each scenario is validated with its own templates.
    fn validate_directory(
        &self,
        dir: &Path,
        files: &mut Vec<PathBuf>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let paths = toml_files(dir, diagnostics);
        files.extend(paths.iter().cloned());
        let sources = read_all(&paths, diagnostics);
        let (scenarios, templates): (Vec<_>, Vec<_>) = sources.into_iter().partition(is_scenario);
        diagnostics.extend(self.load_templates(&templates).diagnostics);
        for scenario in scenarios {
            let load = self.scenario_pipeline(scenario.path());
            files.extend(load.files);
            diagnostics.extend(load.diagnostics);
        }
    }
}

/// Reads the template files under the scenario's `template_paths`, skipping scenario files.
fn collect_templates(
    scenario: &SourceFile,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Vec<SourceFile>, Vec<PathBuf>, bool) {
    let base = scenario.path().parent().unwrap_or_else(|| Path::new(""));
    let mut paths = Vec::new();
    let mut incomplete = false;
    for (entry, origin) in template_paths(scenario) {
        let dir = base.join(&entry);
        let before = diagnostics.len();
        paths.extend(toml_files(&dir, diagnostics));
        if diagnostics.len() > before {
            incomplete = true;
            for d in diagnostics.iter_mut().skip(before) {
                d.location = Some(origin.location());
            }
        }
    }
    paths.sort();
    paths.dedup();
    paths.retain(|p| p != scenario.path());
    let before = diagnostics.len();
    let sources = read_all(&paths, diagnostics);
    incomplete |= diagnostics.len() > before;
    let (scenarios, templates): (Vec<_>, Vec<_>) = sources.into_iter().partition(is_scenario);
    let files = templates
        .iter()
        .chain(&scenarios)
        .map(|f| f.path().to_path_buf())
        .collect();
    (templates, files, incomplete)
}

fn has_syntax_errors(file: &SourceFile) -> bool {
    crate::value::parse(file)
        .1
        .iter()
        .any(|d| d.code == DiagnosticCode::Syntax)
}
