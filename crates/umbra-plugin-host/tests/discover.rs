//! AT-13 (discovery rows): the ADR 0022 package layout and its deterministic order (REQ-P-10, ADR 0022
//! amendment, ASM-121). Packages hold garbage library files: discovery never opens them. Step 4 of M4
//! (WI-8 stub).

#[macro_use]
mod support;

use std::path::Path;

use support::{garbage_package, lib_name, tmp_dir};
use umbra_plugin_host::{PluginError, PluginHost};

fn names(pkgs: &[umbra_plugin_host::PluginPackage]) -> Vec<String> {
    pkgs.iter()
        .map(|p| {
            let parent = p.dir.parent().and_then(Path::file_name).unwrap_or_default();
            format!(
                "{}/{}",
                parent.to_string_lossy(),
                p.dir.file_name().unwrap_or_default().to_string_lossy()
            )
        })
        .collect()
}

#[test]
fn packages_in_path_then_utf8_name_order() {
    let root = tmp_dir("order");
    let first = root.join("first");
    let second = root.join("second");
    for p in ["pkg_b", "pkg_a", "pkg_\u{e9}", "pkg_B"] {
        garbage_package(&first, p, None);
    }
    garbage_package(&second, "pkg_a", None);

    let pkgs = PluginHost::discover(&[first, second]).expect("discover");

    assert_eq!(
        names(&pkgs),
        [
            "first/pkg_B",
            "first/pkg_a",
            "first/pkg_b",
            "first/pkg_\u{e9}",
            "second/pkg_a"
        ],
        "path order, then subdirectory name by UTF-8 bytes"
    );
}

#[cfg(unix)]
#[test]
fn non_utf8_package_name_is_discovery_error() {
    use std::os::unix::ffi::OsStrExt;
    let root = tmp_dir("non-utf8");
    let bad = root.join(std::ffi::OsStr::from_bytes(b"pkg_\xFF"));
    std::fs::create_dir_all(&bad).expect("dir");
    std::fs::write(bad.join(lib_name("x")), b"x").expect("lib");

    let err = PluginHost::discover(&[root]).expect_err("non-UTF-8 name");

    assert!(matches!(err, PluginError::Discovery { .. }), "{err:?}");
    assert!(err.to_string().contains("UTF-8"), "{err}");
}

#[cfg(unix)]
#[test]
fn symlinked_package_uses_canonical_library_path() {
    let root = tmp_dir("symlink");
    let real = tmp_dir("symlink-real");
    garbage_package(&real, "actual", None);
    let plugins = root.join("plugins");
    std::fs::create_dir_all(&plugins).expect("plugins");
    std::os::unix::fs::symlink(real.join("actual"), plugins.join("linked")).expect("symlink");

    let pkgs = PluginHost::discover(&[plugins]).expect("discover");

    assert_eq!(pkgs.len(), 1);
    let canonical =
        std::fs::canonicalize(real.join("actual").join(lib_name("garbage"))).expect("canonical");
    assert_eq!(pkgs[0].library, canonical);
}

#[test]
fn zero_libraries_in_a_package_is_an_error() {
    let root = tmp_dir("zero");
    std::fs::create_dir_all(root.join("empty")).expect("dir");
    std::fs::write(root.join("empty").join("README.txt"), "no library").expect("file");

    let err = PluginHost::discover(&[root]).expect_err("no library");

    assert!(matches!(err, PluginError::Discovery { .. }), "{err:?}");
    assert!(
        err.to_string().contains("empty"),
        "names the package: {err}"
    );
}

#[test]
fn two_libraries_in_a_package_is_an_error() {
    let root = tmp_dir("two");
    let dir = garbage_package(&root, "double", None);
    std::fs::write(dir.join(lib_name("second")), b"x").expect("lib");

    let err = PluginHost::discover(&[root]).expect_err("two libraries");

    assert!(matches!(err, PluginError::Discovery { .. }), "{err:?}");
    assert!(
        err.to_string().contains("double"),
        "names the package: {err}"
    );
}

#[test]
fn missing_plugin_dir_is_an_error() {
    let root = tmp_dir("missing");

    let err = PluginHost::discover(&[root.join("nope")]).expect_err("missing directory");

    assert!(matches!(err, PluginError::Discovery { .. }), "{err:?}");
    assert!(err.to_string().contains("nope"), "names the path: {err}");
}

#[test]
fn manifest_optional() {
    let root = tmp_dir("optional");
    garbage_package(&root, "with", Some("schema_version = 1\n"));
    garbage_package(&root, "without", None);

    let pkgs = PluginHost::discover(&[root]).expect("discover");

    assert_eq!(pkgs.len(), 2);
    assert!(pkgs[0].manifest.is_some(), "with: {:?}", pkgs[0]);
    assert!(pkgs[1].manifest.is_none(), "without: {:?}", pkgs[1]);
}

#[test]
fn library_is_matched_by_extension_only_and_loose_files_are_ignored() {
    let root = tmp_dir("extension");
    let dir = root.join("odd");
    std::fs::create_dir_all(&dir).expect("dir");
    let weird = format!("any.thing.{}", std::env::consts::DLL_EXTENSION);
    std::fs::write(dir.join(&weird), b"x").expect("lib");
    std::fs::write(dir.join("notes.txt"), b"x").expect("notes");
    std::fs::write(root.join("stray.so"), b"x").expect("stray file in the plugin dir");

    let pkgs = PluginHost::discover(&[root]).expect("discover");

    assert_eq!(pkgs.len(), 1);
    assert_eq!(
        pkgs[0].library.file_name().and_then(|n| n.to_str()),
        Some(weird.as_str())
    );
}
