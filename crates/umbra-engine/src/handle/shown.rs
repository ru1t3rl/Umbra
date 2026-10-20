//! Display-form paths (ADR 0026, ASM-173): with a confinement, nothing the engine answers names
//! an absolute path. Without one, answers are unchanged.

use std::path::Path;

use umbra_templates::{Diagnostic, Diagnostics, Location};

use super::thread::Worker;
use crate::scenario::ScenarioError;
use crate::session::SessionError;

impl Worker {
    /// The path as shown to a client.
    pub(super) fn shown_path(&self, path: &Path) -> String {
        match &self.settings.load.confinement {
            Some(confinement) => confinement.display(path),
            None => path.display().to_string(),
        }
    }

    fn shown_location(&self, location: &mut Location) {
        if self.settings.load.confinement.is_some() {
            location.path = self.shown_path(&location.path).into();
        }
    }

    /// The diagnostic with its files and quoted paths in display form.
    pub(super) fn shown_diagnostic(&self, mut diagnostic: Diagnostic) -> Diagnostic {
        let Some(confinement) = &self.settings.load.confinement else {
            return diagnostic;
        };
        diagnostic.message = confinement.scrub(&diagnostic.message);
        if let Some(location) = &mut diagnostic.location {
            self.shown_location(location);
        }
        for note in &mut diagnostic.notes {
            note.message = confinement.scrub(&note.message);
            if let Some(location) = &mut note.location {
                self.shown_location(location);
            }
        }
        diagnostic
    }

    /// A load error whose diagnostics name files in display form.
    pub(super) fn shown_session_error(&self, error: SessionError) -> SessionError {
        match error {
            SessionError::Scenario(ScenarioError::Invalid(diagnostics)) => {
                let shown = diagnostics
                    .iter()
                    .map(|d| self.shown_diagnostic(d.clone()))
                    .collect();
                let diagnostics = Diagnostics::new(shown).unwrap_or(diagnostics);
                SessionError::Scenario(ScenarioError::Invalid(diagnostics))
            }
            other => other,
        }
    }
}
