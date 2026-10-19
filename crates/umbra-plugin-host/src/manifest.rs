//! `plugin.toml` (ADR 0022, REQ-P-9, REQ-P-10): read and checked before the library is opened.
//!
//! ```toml
//! schema_version = 1
//! id = "example.plugin.wander"   # PluginId (ADR 0002)
//! version = "0.1.0"              # semantic version
//! abi = "0.0"                    # "<major>.<minor>"
//! name = "Wander example"        # optional
//! ```
//! Unknown keys are errors. An ABI this host cannot load (major differs, or minor newer) is
//! rejected before the library is opened; after loading, the descriptor's id, version and ABI must
//! equal the manifest's ([`Manifest::check`]).

use std::collections::BTreeMap;
use std::path::Path;

use toml::de::{DeTable, DeValue};

use umbra_core::{PluginId, Version};
use umbra_plugin_api::{UMBRA_ABI_VERSION_MAJOR, UMBRA_ABI_VERSION_MINOR};

use crate::descriptor::Descriptor;
use crate::error::PluginError;

/// The only supported `schema_version`.
const SCHEMA_VERSION: i64 = 1;
/// Every key `plugin.toml` may have.
const KEYS: [&str; 5] = ["schema_version", "id", "version", "abi", "name"];

/// A parsed `plugin.toml`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Manifest {
    id: PluginId,
    version: Version,
    abi: (u32, u32),
}

impl Manifest {
    /// Reads and parses `path` (syntax, keys and values).
    pub(crate) fn read(path: &Path) -> Result<Self, PluginError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| manifest_error(path, format!("cannot read: {e}")))?;
        Self::parse(path, &text)
    }

    fn parse(path: &Path, text: &str) -> Result<Self, PluginError> {
        let fail = |message: String| manifest_error(path, message);
        let document =
            DeTable::parse(text).map_err(|e| fail(format!("invalid TOML: {}", e.message())))?;
        let mut table: BTreeMap<&str, &DeValue<'_>> = BTreeMap::new();
        for (key, value) in document.get_ref() {
            let key: &str = key.get_ref();
            if !KEYS.contains(&key) {
                return Err(fail(format!(
                    "unknown key `{key}`; expected one of {}",
                    KEYS.join(", ")
                )));
            }
            table.insert(key, value.get_ref());
        }
        let text_of = |key: &str| -> Result<&str, PluginError> {
            match table.get(key) {
                None => Err(fail(format!("missing key `{key}`"))),
                Some(v) => v
                    .as_str()
                    .ok_or_else(|| fail(format!("`{key}` must be a string"))),
            }
        };
        let schema_version = table
            .get("schema_version")
            .ok_or_else(|| fail("missing key `schema_version`".to_owned()))?;
        let number = schema_version
            .as_integer()
            .and_then(|i| i64::from_str_radix(i.as_str(), i.radix()).ok());
        if number != Some(SCHEMA_VERSION) {
            return Err(fail(format!(
                "`schema_version` must be the integer {SCHEMA_VERSION}"
            )));
        }
        let id = text_of("id")?;
        let id = PluginId::new(id).map_err(|e| fail(format!("`id`: {e}")))?;
        let version = text_of("version")?;
        let version = Version::parse(version).map_err(|e| {
            fail(format!(
                "`version` {version:?} is not a semantic version: {e}"
            ))
        })?;
        let abi = text_of("abi")?;
        let abi = parse_abi(abi)
            .ok_or_else(|| fail(format!("`abi` {abi:?} must be \"<major>.<minor>\"")))?;
        if table.get("name").is_some_and(|name| !name.is_str()) {
            return Err(fail("`name` must be a string".to_owned()));
        }
        Ok(Self { id, version, abi })
    }

    /// REQ-P-9 before opening: the manifest's ABI must be loadable by this host.
    pub(crate) fn check_abi(&self, origin: &str) -> Result<(), PluginError> {
        let host = (UMBRA_ABI_VERSION_MAJOR, UMBRA_ABI_VERSION_MINOR);
        if self.abi.0 == host.0 && self.abi.1 <= host.1 {
            Ok(())
        } else {
            Err(PluginError::AbiMismatch {
                origin: origin.to_owned(),
                field: "plugin.toml abi".to_owned(),
                plugin: self.abi,
                host,
            })
        }
    }

    /// ADR 0022: after loading (and again on a reused image), the descriptor must match.
    pub(crate) fn check(&self, origin: &str, descriptor: &Descriptor) -> Result<(), PluginError> {
        let mismatch = |field: &'static str, manifest: String, descriptor: String| {
            Err(PluginError::ManifestMismatch {
                origin: origin.to_owned(),
                field,
                manifest,
                descriptor,
            })
        };
        if self.id != descriptor.id {
            return mismatch("id", self.id.to_string(), descriptor.id.to_string());
        }
        if self.version != descriptor.version {
            return mismatch(
                "version",
                self.version.to_string(),
                descriptor.version.to_string(),
            );
        }
        if self.abi != descriptor.abi {
            return mismatch(
                "abi",
                format!("{}.{}", self.abi.0, self.abi.1),
                format!("{}.{}", descriptor.abi.0, descriptor.abi.1),
            );
        }
        Ok(())
    }
}

fn manifest_error(path: &Path, message: String) -> PluginError {
    PluginError::Manifest {
        path: path.to_path_buf(),
        message,
    }
}

/// `"<major>.<minor>"` with plain decimal digits.
fn parse_abi(text: &str) -> Option<(u32, u32)> {
    let (major, minor) = text.split_once('.')?;
    let number = |s: &str| {
        (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse().ok())
            .flatten()
    };
    Some((number(major)?, number(minor)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str =
        "schema_version = 1\nid = \"test.plugin.unit\"\nversion = \"1.2.3\"\nabi = \"0.0\"\n";

    fn parse(text: &str) -> Result<Manifest, PluginError> {
        Manifest::parse(Path::new("pkg/plugin.toml"), text)
    }

    #[test]
    fn a_valid_manifest_parses() {
        let m = parse(VALID).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(m.id.as_str(), "test.plugin.unit");
        assert_eq!(m.version, Version::new(1, 2, 3));
        assert_eq!(m.abi, (0, 0));
        assert!(m.check_abi("o").is_ok());
        let named = parse(&format!("{VALID}name = \"Unit\"\n"));
        assert!(named.is_ok());
    }

    #[test]
    fn abi_text_is_strict() {
        assert_eq!(parse_abi("0.0"), Some((0, 0)));
        assert_eq!(parse_abi("12.34"), Some((12, 34)));
        for bad in ["", "0", "0.", ".0", "0.0.0", "+1.0", "0.-1", " 0.0", "a.b"] {
            assert_eq!(parse_abi(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn wrong_types_are_errors() {
        let err = parse(&VALID.replace("\"test.plugin.unit\"", "3"));
        assert!(err.is_err_and(|e| e.to_string().contains("`id` must be a string")));
        let err = parse(&VALID.replace("schema_version = 1", "schema_version = \"1\""));
        assert!(err.is_err_and(|e| e.to_string().contains("schema_version")));
    }
}
