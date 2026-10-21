//! The required-minor rule (ADR 0033, clarified 2026-10-21; ASM-403, ASM-406).
//!
//! A capability kind's minor is the newest ABI minor that changed any struct on its call surface. Every
//! struct the SDK writes for a capability (its descriptor entry, its vtable, and the structs it writes
//! during the capability's calls, such as intents and values) carries that minor and its layout; the
//! descriptor declares the highest minor of its capabilities. So a plugin built with a newer SDK needs a
//! newer host only when it exports a newer kind. A minor that grows a struct on a kind's call surface
//! bumps the kind's minor here (or the SDK keeps writing the old prefix); the surface table in the tests
//! below pins this.

use crate::abi::UMBRA_ABI_VERSION_MAJOR;

/// The minor of the behavior kind: the newest minor that changed a struct of its call surface.
pub(crate) const BEHAVIOR_MINOR: u32 = 0;

/// The packed `abi_version` of minor `minor` of this SDK's major.
pub(crate) const fn abi_version(minor: u32) -> u32 {
    (UMBRA_ABI_VERSION_MAJOR << 16) | minor
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every struct of the behavior call surface with the minor of its last layout change
    /// (`docs/plugin-abi-changelog.md`). A minor that changes one of them updates its row here, and
    /// then `BEHAVIOR_MINOR` must follow (or the SDK must keep writing the old prefix).
    const BEHAVIOR_SURFACE: [(&str, u32); 10] = [
        ("UmbraCapability", 0),
        ("UmbraBehaviorVTable", 0),
        ("UmbraBehaviorBatch", 0),
        ("UmbraBehaviorItem", 0),
        ("UmbraEntityView", 0),
        ("UmbraContact", 0),
        ("UmbraIntent", 0),
        ("UmbraIntentSink", 0),
        ("UmbraValue", 0),
        ("UmbraValueSink", 0),
    ];

    /// Whether a kind of minor `kind_minor` covers every struct of `surface`.
    fn covers(kind_minor: u32, surface: &[(&str, u32)]) -> bool {
        surface.iter().all(|&(_, changed)| changed <= kind_minor)
    }

    #[test]
    fn the_behavior_minor_covers_its_call_surface() {
        assert!(
            covers(BEHAVIOR_MINOR, &BEHAVIOR_SURFACE),
            "BEHAVIOR_MINOR {BEHAVIOR_MINOR} is older than a struct of its call surface"
        );
        // Control: a surface struct changed in a newer minor is not covered.
        assert!(!covers(0, &[("UmbraIntent", 1)]));
        let mut names: Vec<&str> = BEHAVIOR_SURFACE.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), BEHAVIOR_SURFACE.len(), "each struct once");
    }

    #[test]
    fn abi_version_packs_this_major_with_the_minor() {
        assert_eq!(abi_version(0) >> 16, UMBRA_ABI_VERSION_MAJOR);
        assert_eq!(abi_version(3) & 0xFFFF, 3);
    }
}
