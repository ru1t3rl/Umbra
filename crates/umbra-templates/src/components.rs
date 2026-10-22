//! Component checks shared by templates and scenarios: registry lookup, parameter schema
//! validation and a trial `Factory::build` (REQ-T-1, REQ-T-3, ASM-058, ASM-063, ASM-064).

use std::path::Path;

use umbra_core::{BuildError, ComponentId, ComponentKind, ParamValue, Registry};

use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::loader::Loader;
use crate::value::{LKind, LTable, LValue, Origin};

impl Loader<'_> {
    /// Checks one component use. Returns the parameters as JSON when the component is registered
    /// and its parameters passed the schema and the trial build; otherwise records diagnostics.
    ///
    /// `params` is the merged, located parameter table; its origin is where a missing parameter
    /// is reported.
    pub(crate) fn check_component(
        &self,
        kind: ComponentKind,
        id: &ComponentId,
        id_at: &Origin,
        params: &LTable,
        out: &mut Vec<Diagnostic>,
    ) -> Option<ParamValue> {
        self.check_component_wrapped(kind, id, id_at, params, out, None)
    }

    /// [`check_component`](Self::check_component) where the factory's input is `wrap(params)`
    /// (a network adapter gets `{"name", "params", "templates"}`, ADR 0030). The schema applies to
    /// `params` alone; a `BuildError` pointer under `/params` is located in the parameter table.
    pub(crate) fn check_component_wrapped(
        &self,
        kind: ComponentKind,
        id: &ComponentId,
        id_at: &Origin,
        params: &LTable,
        out: &mut Vec<Diagnostic>,
        wrap: Option<&dyn Fn(&ParamValue) -> ParamValue>,
    ) -> Option<ParamValue> {
        if !self.check_registered(kind, id, id_at, out) {
            return None;
        }
        let schema = self.schemas.get(&(kind, id.clone()))?;
        let context = format!("{} `{id}`", kind.noun());
        let found = schema.validate(&params.clone().into_value(), &context);
        if !found.is_empty() {
            out.extend(found);
            return None;
        }
        let json = params.to_param();
        let input = wrap.map_or_else(|| json.clone(), |w| w(&json));
        match self.trial_build(kind, id, &input) {
            Ok(()) => Some(json),
            Err(error) => {
                let pointer = match wrap {
                    Some(_) => error
                        .param
                        .as_deref()
                        .and_then(|p| p.strip_prefix(umbra_core::net::PARAMS_POINTER)),
                    None => error.param.as_deref(),
                };
                let at = locate(params, pointer);
                let message = format!("{context}: {}", error.message);
                out.push(Diagnostic::new(
                    DiagnosticCode::ParamInvalid,
                    message,
                    Some(at.location()),
                ));
                None
            }
        }
    }

    /// Checks the parameters a template or override writes itself against the schema, key by
    /// key, without the merge: a missing required parameter may come from elsewhere, so
    /// `missing_param` is dropped, and no trial build runs (ASM-056).
    pub(crate) fn check_own_params(
        &self,
        kind: ComponentKind,
        id: &ComponentId,
        params: Option<&LTable>,
        out: &mut Vec<Diagnostic>,
    ) {
        let (Some(params), Some(schema)) = (params, self.schemas.get(&(kind, id.clone()))) else {
            return;
        };
        let context = format!("{} `{id}`", kind.noun());
        out.extend(
            schema
                .validate(&params.clone().into_value(), &context)
                .into_iter()
                .filter(|d| d.code != DiagnosticCode::MissingParam),
        );
    }

    /// Whether `id` is registered as a `kind`; if not, records `unknown_id`.
    pub(crate) fn check_registered(
        &self,
        kind: ComponentKind,
        id: &ComponentId,
        at: &Origin,
        out: &mut Vec<Diagnostic>,
    ) -> bool {
        let registered = self.schemas.contains_key(&(kind, id.clone()));
        let silent = kind == ComponentKind::Behavior && self.scope.silent.contains(id);
        if !registered && !silent {
            out.push(self.unknown_id(kind, id, at));
        }
        registered
    }

    /// An `unknown_id` diagnostic with a note listing what is registered. An ID that is
    /// registered under another kind says so (IDs are global, ASM-063).
    pub(crate) fn unknown_id(
        &self,
        kind: ComponentKind,
        id: &ComponentId,
        at: &Origin,
    ) -> Diagnostic {
        let message = match self.registry.kind_of(id) {
            Some(other) if other != kind => {
                format!(
                    "`{id}` is {} {}, not {} {}",
                    article(other.noun()),
                    other.noun(),
                    article(kind.noun()),
                    kind.noun()
                )
            }
            _ => format!("{} `{id}` is not registered", kind.noun()),
        };
        let ids = self.registry.ids(kind);
        let note = if ids.is_empty() {
            "none are registered in this build".to_string()
        } else {
            let list: Vec<&str> = ids.iter().map(|i| i.as_str()).collect();
            format!("registered {}s: {}", kind.noun(), list.join(", "))
        };
        let mut found = Diagnostic::new(DiagnosticCode::UnknownId, message, Some(at.location()))
            .with_note(note, None);
        if kind == ComponentKind::Behavior {
            for path in &self.scope.unreadable {
                found = found.with_note(syntax_note(path), None);
            }
        }
        found
    }

    fn trial_build(
        &self,
        kind: ComponentKind,
        id: &ComponentId,
        params: &ParamValue,
    ) -> Result<(), BuildError> {
        let registry: &Registry = self.registry;
        match kind {
            ComponentKind::MotionModel => registry
                .motion_model(id)
                .map_or(Ok(()), |f| f.build(params).map(drop)),
            ComponentKind::Terrain => registry
                .terrain(id)
                .map_or(Ok(()), |f| f.build(params).map(drop)),
            // Graph leaves, registered behaviors and sensors are dry-run through their factories.
            // A graph of the load has no factory: its leaves are dry-run where they are written.
            ComponentKind::Behavior => registry
                .behavior(id)
                .map_or(Ok(()), |f| f.build(params).map(drop)),
            ComponentKind::Condition => registry
                .condition(id)
                .map_or(Ok(()), |f| f.build(params).map(drop)),
            ComponentKind::Action => registry
                .action(id)
                .map_or(Ok(()), |f| f.build(params).map(drop)),
            ComponentKind::Sensor => registry
                .sensor(id)
                .map_or(Ok(()), |f| f.build(params).map(drop)),
            // Building a network adapter configuration opens nothing (ASM-291 a).
            ComponentKind::NetworkAdapter => registry
                .network_adapter(id)
                .map_or(Ok(()), |f| f.build(params).map(drop)),
            // `ComponentKind` is `non_exhaustive`: a kind added later must get a dry run here.
            _ => Err(BuildError::new("component kind not supported")),
        }
    }
}

/// Drops the `missing_param` diagnostics located at the root `params` table `root`: the loader
/// recomputes those for behaviors (binding-only params included). Nested ones stay (ASM-107).
pub(crate) fn drop_root_missing(found: &mut Vec<Diagnostic>, root: &Origin) {
    let at = root.location();
    found.retain(|d| d.code != DiagnosticCode::MissingParam || d.location.as_ref() != Some(&at));
}

/// The note of an `unknown_*` error for a file with syntax errors that may define the name
/// (ASM-054 rule 3).
pub(crate) fn syntax_note(path: &Path) -> String {
    format!(
        "may be defined in {}, which has syntax errors",
        path.display()
    )
}

/// "a" or "an" before `noun`.
pub(crate) fn article(noun: &str) -> &'static str {
    if noun.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    }
}

/// Resolves an RFC 6901 pointer into `params` to the origin of its deepest existing ancestor
/// (ASM-064). No pointer, or a pointer that leaves the tree at once, is the table itself.
fn locate(params: &LTable, pointer: Option<&str>) -> Origin {
    let mut best = params.origin().clone();
    let Some(rest) = pointer.and_then(|p| p.strip_prefix('/')) else {
        return best;
    };
    let mut current: Option<&LValue> = None;
    let mut table = Some(params);
    for raw in rest.split('/') {
        let token = raw.replace("~1", "/").replace("~0", "~");
        let next = match (table, current.map(LValue::kind)) {
            (Some(t), _) => t.get(&token),
            (None, Some(LKind::Array(items))) => {
                token.parse::<usize>().ok().and_then(|i| items.get(i))
            }
            _ => None,
        };
        let Some(value) = next else { break };
        best = value.origin().clone();
        table = match value.kind() {
            LKind::Table(t) => Some(t),
            _ => None,
        };
        current = Some(value);
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behaviors and sensors have no registry entries in M2 (ASM-055), so `trial_build` has no
    /// real dry run for them. This fails the moment M3 registers one.
    #[test]
    fn behaviors_and_sensors_have_no_registry_entries_yet() {
        let registry = Registry::new();

        for kind in [ComponentKind::Behavior, ComponentKind::Sensor] {
            assert_eq!(
                registry.ids(kind).len(),
                0,
                "M3: replace the Behavior|Sensor arm in trial_build with a real dry run"
            );
        }
    }
    use crate::source::SourceFile;
    use crate::value::parse;

    fn params(text: &str) -> LTable {
        let (table, _) = parse(&SourceFile::new("p.toml", text));
        let value = table.and_then(|t| t.get("p").cloned());
        match value.map(|v| v.kind().clone()) {
            Some(LKind::Table(t)) => t,
            _ => panic!("no table `p`"),
        }
    }

    fn col(origin: &Origin) -> u32 {
        origin.location().column
    }

    #[test]
    fn pointer_resolves_to_the_deepest_existing_ancestor() {
        let p = params("p = { a = 1, list = [10, 20], t = { k = 1 } }\n");

        assert_eq!(col(&locate(&p, Some("/a"))), 11);
        assert_eq!(col(&locate(&p, Some("/list/1"))), 26);
        assert_eq!(
            col(&locate(&p, Some("/t/missing"))),
            col(&locate(&p, Some("/t")))
        );
        assert_eq!(col(&locate(&p, Some("/nope"))), col(p.origin()));
        assert_eq!(col(&locate(&p, None)), col(p.origin()));
        assert_eq!(col(&locate(&p, Some("a"))), col(p.origin()));
    }
}
