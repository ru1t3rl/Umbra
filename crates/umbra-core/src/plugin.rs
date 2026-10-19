//! Plugin-related core types (REQ-P-7, ADR 0021).

/// What an engine does when a plugin capability faults (error code, panic, contract breach).
///
/// The fault state belongs to one engine (ASM-118). Names are the scenario/config spellings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum PluginFaultPolicy {
    /// The tick fails and the run stops (the default).
    #[default]
    Abort,
    /// The faulted capability is disabled and its entities fail their behavior each tick.
    DisableCapability,
}

impl PluginFaultPolicy {
    /// Parses `"abort"` or `"disable_capability"`; anything else is `None`.
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "abort" => Some(Self::Abort),
            "disable_capability" => Some(Self::DisableCapability),
            _ => None,
        }
    }

    /// The canonical name, the inverse of [`PluginFaultPolicy::from_name`].
    pub fn name(self) -> &'static str {
        match self {
            Self::Abort => "abort",
            Self::DisableCapability => "disable_capability",
        }
    }
}
