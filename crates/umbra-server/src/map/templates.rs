//! Templates: resolved templates to wire summaries and details (WI-13).
//!
//! The engine writes `source` relative to the scenario root (ASM-194); it is passed on as it is.

use umbra_api_types as api;
use umbra_engine::{ComponentId, ParamValue, ResolvedTemplate};

fn component_ref(id: &ComponentId, params: &ParamValue) -> api::ComponentRef {
    api::ComponentRef {
        id: id.as_str().to_owned(),
        params: params.clone(),
    }
}

/// The wire summary of a template.
#[must_use]
pub fn template_summary(template: &ResolvedTemplate) -> api::TemplateSummary {
    api::TemplateSummary {
        id: template.id.as_str().to_owned(),
        description: template.description.clone(),
        motion: template.motion.id.as_str().to_owned(),
        behavior: template.behavior.as_ref().map(|b| b.id.as_str().to_owned()),
        source: template.source.to_string_lossy().into_owned(),
    }
}

/// The wire detail of a template, inheritance applied.
#[must_use]
pub fn template_detail(template: &ResolvedTemplate) -> api::TemplateDetail {
    api::TemplateDetail {
        id: template.id.as_str().to_owned(),
        chain: template
            .chain
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect(),
        description: template.description.clone(),
        motion: component_ref(&template.motion.id, &template.motion.params),
        behavior: template
            .behavior
            .as_ref()
            .map(|b| component_ref(&b.id, &b.params)),
        sensors: template
            .sensors
            .iter()
            .map(|s| component_ref(&s.id, &s.params))
            .collect(),
        source: template.source.to_string_lossy().into_owned(),
    }
}
