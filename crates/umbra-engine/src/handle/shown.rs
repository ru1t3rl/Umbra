//! Display-form paths (ADR 0026, ASM-173): with a confinement, nothing the engine answers names
//! an absolute path. Without one, answers are unchanged.

use std::path::Path;

use umbra_templates::{Diagnostic, Diagnostics, Location};

use super::thread::Worker;
use crate::bus::StopReason;
use crate::confine::Confinement;
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

    /// The stop reason with the messages that can quote a path (a kernel error, a plugin fault)
    /// written in display form (ASM-194, ASM-197), so status and frames leak none.
    pub(super) fn shown_stop_reason(&self, reason: StopReason) -> StopReason {
        scrub_stop_reason(self.settings.load.confinement.as_ref(), reason)
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

/// `reason` with its messages scrubbed by `confinement`; unchanged without one.
fn scrub_stop_reason(confinement: Option<&Confinement>, reason: StopReason) -> StopReason {
    let Some(confinement) = confinement else {
        return reason;
    };
    match reason {
        StopReason::Error(message) => StopReason::Error(confinement.scrub(&message)),
        StopReason::PluginFault(mut fault) => {
            fault.message = confinement.scrub(&fault.message);
            StopReason::PluginFault(fault)
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn confinement() -> Confinement {
        Confinement {
            root: "/srv/scn".into(),
            plugin_roots: vec!["/opt/plugins".into()],
        }
    }

    #[test]
    fn an_error_message_loses_the_roots() {
        let shown = scrub_stop_reason(
            Some(&confinement()),
            StopReason::Error("cannot read /srv/scn/graphs/a.toml and /opt/plugins/p.so".into()),
        );
        assert_eq!(
            shown,
            StopReason::Error("cannot read graphs/a.toml and plugins[0]/p.so".into())
        );
    }

    #[test]
    fn without_a_confinement_and_for_other_reasons_nothing_changes() {
        let quoted = StopReason::Error("/srv/scn/x".into());
        assert_eq!(scrub_stop_reason(None, quoted.clone()), quoted);
        assert_eq!(
            scrub_stop_reason(Some(&confinement()), StopReason::Command),
            StopReason::Command
        );
        assert_eq!(
            scrub_stop_reason(Some(&confinement()), StopReason::Duration),
            StopReason::Duration
        );
    }
}
