//! Component checks shared by templates and scenarios: registry lookup, parameter schema
//! validation and a trial `Factory::build` (REQ-T-1, REQ-T-3, ASM-058, ASM-063, ASM-064).

use cgf_core::{BuildError, ComponentId, ComponentKind, ParamValue, Registry};

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
        match self.trial_build(kind, id, &json) {
            Ok(()) => Some(json),
            Err(error) => {
                let at = locate(params, error.param.as_deref());
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

    /// Whether `id` is registered as a `kind`; if not, records `unknown_id`.
    pub(crate) fn check_registered(
        &self,
        kind: ComponentKind,
        id: &ComponentId,
        at: &Origin,
        out: &mut Vec<Diagnostic>,
    ) -> bool {
        let registered = self.schemas.contains_key(&(kind, id.clone()));
        if !registered {
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
                format!("`{id}` is a {}, not a {}", other.noun(), kind.noun())
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
        Diagnostic::new(DiagnosticCode::UnknownId, message, Some(at.location()))
            .with_note(note, None)
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
            // No factories for these kinds until M3 (ASM-055); `Loader::new` never compiles a
            // schema for them, so they cannot reach here registered.
            ComponentKind::Behavior | ComponentKind::Sensor => Ok(()),
            _ => Err(BuildError::new("component kind not supported")),
        }
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
