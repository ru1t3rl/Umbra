//! `validate --json` warnings (M6 WI-8, ASM-290). The standard registry holds no network adapter
//! yet, so the non-empty case is tested on the document builder.

use umbra_engine::scenario::{Diagnostic, DiagnosticCode};

use crate::output::validation_json;

fn warning() -> Diagnostic {
    Diagnostic::warning(DiagnosticCode::SuspiciousValue, "short timeout", None)
}

fn document(files: &[&str], errors: &[Diagnostic], warnings: &[Diagnostic]) -> serde_json::Value {
    let files: Vec<std::path::PathBuf> = files.iter().map(Into::into).collect();
    serde_json::from_str(&validation_json(&files, errors, warnings)).unwrap_or_default()
}

#[test]
fn warnings_are_listed_in_the_diagnostic_shape_and_do_not_affect_valid() {
    let doc = document(&["s.toml"], &[], &[warning()]);

    assert_eq!(doc["valid"], true);
    assert_eq!(doc["diagnostics"], serde_json::json!([]));
    assert_eq!(doc["warnings"].as_array().map(Vec::len), Some(1));
    assert_eq!(doc["warnings"][0]["code"], "suspicious_value");
    assert_eq!(doc["warnings"][0]["message"], "short timeout");
    assert_eq!(doc["warnings"][0]["notes"], serde_json::json!([]));
}

#[test]
fn errors_still_make_the_document_invalid_next_to_warnings() {
    let error = Diagnostic::new(DiagnosticCode::InvalidValue, "bad", None);

    let doc = document(&[], &[error], &[warning()]);

    assert_eq!(doc["valid"], false);
    assert_eq!(doc["warnings"].as_array().map(Vec::len), Some(1));
}
