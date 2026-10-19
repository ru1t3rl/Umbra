//! `umbra-api-types`: the wire types of the Umbra control API and stream.
//!
//! Responsibility: plain `serde` + `utoipa` data types shared by `umbra-server` and the generated
//! contract files (REQ-X-4, REQ-X-6, D-12, ADR 0025). It depends on no workspace crate (ASM-008) so
//! the contract cannot leak engine types. M5 WI-1 only scaffolds it; the DTOs arrive in WI-11.
//!
//! Dependency rule (section 4, REQ-A-1..4): no workspace dependencies. It contains no `unsafe` code.

#![forbid(unsafe_code)]

/// Version of the REST and stream contract (semver, additive changes only; ADR 0026).
///
/// It is the `info.version` of the generated `OpenAPI` document, not the crate version.
pub const API_VERSION: &str = "1.0.0";

#[cfg(test)]
mod tests {
    use super::API_VERSION;

    #[test]
    fn api_version_is_semver_triple_starting_at_one() {
        let parts: Vec<&str> = API_VERSION.split('.').collect();
        assert_eq!(parts.len(), 3, "{API_VERSION}");
        assert!(
            parts.iter().all(|p| p.parse::<u32>().is_ok()),
            "{API_VERSION}"
        );
        assert_eq!(parts[0], "1");
    }
}
