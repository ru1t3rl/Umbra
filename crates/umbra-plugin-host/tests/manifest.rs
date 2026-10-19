//! AT-4 (manifest rows): `plugin.toml` is checked before the library is opened (ADR 0022, REQ-P-9,
//! REQ-P-10). The library file is garbage, so "rejected without opening" is observable: an ABI problem
//! must surface as `AbiMismatch`/`Manifest`, never as `Open`. Step 4 of M4 (WI-8 stub).

#[macro_use]
mod support;

use support::{garbage_package, tmp_dir};
use umbra_plugin_host::{HostOptions, PluginError, PluginHost};

const VALID: &str =
    "schema_version = 1\nid = \"test.plugin.manifest\"\nversion = \"0.1.0\"\nabi = \"0.0\"\n";

fn load_garbage(manifest: Option<&str>) -> (PluginError, std::path::PathBuf) {
    let root = tmp_dir("manifest");
    garbage_package(&root, "pkg", manifest);
    let pkgs = PluginHost::discover(&[root]).unwrap_or_else(|e| panic!("discover: {e}"));
    let mut host = PluginHost::new(HostOptions::default());
    let err = host
        .load(&pkgs[0])
        .err()
        .unwrap_or_else(|| panic!("garbage library cannot load"));
    (err, pkgs[0].dir.clone())
}

macro_rules! manifest_error {
    ($name:ident, $toml:expr, $needle:expr) => {
        #[test]
        #[ignore = "WI-8"]
        fn $name() {
            let (err, dir) = load_garbage(Some($toml));

            assert!(matches!(err, PluginError::Manifest { .. }), "{err:?}");
            let msg = err.to_string();
            assert!(msg.contains("plugin.toml"), "names the file: {msg}");
            assert!(msg.contains($needle), "names {:?}: {msg}", $needle);
            let _ = dir;
        }
    };
}

#[test]
#[ignore = "WI-8"]
fn given_a_valid_manifest_and_a_garbage_library_then_open_error() {
    let (err, _) = load_garbage(Some(VALID));

    assert!(matches!(err, PluginError::Open { .. }), "{err:?}");
}

#[test]
#[ignore = "WI-8"]
fn given_no_manifest_and_a_garbage_library_then_open_error() {
    let (err, _) = load_garbage(None);

    assert!(matches!(err, PluginError::Open { .. }), "{err:?}");
}

#[test]
#[ignore = "WI-8"]
fn given_a_manifest_with_another_abi_major_then_rejected_without_opening() {
    let (err, _) = load_garbage(Some(&VALID.replace("0.0", "1.0")));

    assert!(matches!(err, PluginError::AbiMismatch { .. }), "{err:?}");
    assert!(err.to_string().contains("ABI"));
}

#[test]
#[ignore = "WI-8"]
fn given_a_manifest_with_a_newer_abi_minor_then_rejected_without_opening() {
    let (err, _) = load_garbage(Some(&VALID.replace("\"0.0\"", "\"0.1\"")));

    assert!(matches!(err, PluginError::AbiMismatch { .. }), "{err:?}");
}

manifest_error!(toml_syntax_error, "schema_version = [", "plugin.toml");
manifest_error!(unknown_key, &format!("{VALID}bogus = 1\n"), "bogus");
manifest_error!(
    missing_schema_version,
    &VALID.replace("schema_version = 1\n", ""),
    "schema_version"
);
manifest_error!(
    unsupported_schema_version,
    &VALID.replace("schema_version = 1", "schema_version = 2"),
    "schema_version"
);
manifest_error!(
    missing_id,
    &VALID.replace("id = \"test.plugin.manifest\"\n", ""),
    "id"
);
manifest_error!(
    missing_version,
    &VALID.replace("version = \"0.1.0\"\n", ""),
    "version"
);
manifest_error!(missing_abi, &VALID.replace("abi = \"0.0\"\n", ""), "abi");
manifest_error!(
    bad_id_grammar,
    &VALID.replace("test.plugin.manifest", "Bad Id"),
    "id"
);
manifest_error!(bad_semver, &VALID.replace("0.1.0", "one"), "version");
manifest_error!(bad_abi_text, &VALID.replace("\"0.0\"", "\"zero\""), "abi");
manifest_error!(
    name_of_the_wrong_type,
    &format!("{VALID}name = 3\n"),
    "name"
);
