//! Protocol-neutrality source test (ADR 0030, ASM-226, ASM-255, ASM-259).
//!
//! `umbra-core`, `umbra-kernel`, `umbra-net` and `umbra-engine` know no DIS vocabulary: the neutral
//! model maps to protocol specifics only inside an adapter. This test scans their `src/**/*.rs`,
//! `build.rs` (code and comments) and the keys of the `[features]` table of their `Cargo.toml`.
//!
//! An identifier or word is split into humps at every non-alphanumeric character (so `_` splits),
//! at lower-to-upper case changes and at the end of an upper-case run (`PduHeader` is `Pdu` and
//! `Header`; `PDUs` stays one hump). A hump matches, ignoring case and trailing digits, the words
//! `dis`, `pdu`, `siso`, `rpr` (also with a trailing `s`); the hump pairs `site id` and
//! `application id` match inside one identifier (`site_id`, `siteId`); the token `1278` matches
//! on its own. So `pdu_len`, `DisMode`, `struct Pdu` and `DIS_MODE` match, while `Display`,
//! `dispatch`, `distance`, `DISCARD`, `redis` and `disallowed` do not. The scanner is unit-tested.

#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};

/// Crates that must stay protocol-neutral (kept equal to `xtask::deps::PROTOCOL_NEUTRAL` by a test
/// in `xtask/tests/check_rules.rs`).
const NEUTRAL_CRATES: &[&str] = &["umbra-core", "umbra-engine", "umbra-kernel", "umbra-net"];

/// Whole-hump words, lower case.
const WORDS: &[&str] = &["dis", "pdu", "siso", "rpr"];

/// True when `next` begins a lower-case word after an upper-case run; a lone plural `s` (`PDUs`)
/// does not.
fn starts_word(next: Option<char>, after: Option<char>) -> bool {
    match (next, after) {
        (Some('s'), a) => a.is_some_and(|a| a.is_ascii_lowercase()),
        (Some(n), _) => n.is_ascii_lowercase(),
        (None, _) => false,
    }
}

/// Splits one alphanumeric token (no `_`) into case humps, lower-cased.
fn humps(token: &str) -> Vec<String> {
    let chars: Vec<char> = token.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let prev = i.checked_sub(1).map(|j| chars[j]);
        let next = chars.get(i + 1).copied();
        let after = chars.get(i + 2).copied();
        let split = c.is_ascii_uppercase()
            && match prev {
                Some(p) if p.is_ascii_lowercase() || p.is_ascii_digit() => true,
                Some(p) if p.is_ascii_uppercase() => {
                    // End of an upper-case run before a word (`PDUHeader`), but `PDUs` stays whole.
                    starts_word(next, after)
                }
                _ => false,
            };
        if split && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        cur.push(c.to_ascii_lowercase());
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn is_word(hump: &str) -> bool {
    let h = hump.trim_end_matches(|c: char| c.is_ascii_digit());
    WORDS
        .iter()
        .any(|w| h == *w || h.strip_suffix('s') == Some(*w))
}

/// Returns the offending words of one text as `(line number from 1, normalized word)`.
fn scan(source: &str) -> Vec<(usize, String)> {
    let mut hits = Vec::new();
    for (i, line) in source.lines().enumerate() {
        let n = i + 1;
        // Identifier runs: letters, digits and `_`.
        for run in line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
            let hs: Vec<String> = run
                .split('_')
                .filter(|t| !t.is_empty())
                .flat_map(humps)
                .collect();
            for h in &hs {
                if is_word(h) {
                    let h = h.trim_end_matches(|c: char| c.is_ascii_digit());
                    let base = h
                        .strip_suffix('s')
                        .filter(|b| WORDS.contains(b))
                        .unwrap_or(h);
                    hits.push((n, base.to_string()));
                }
            }
            for pair in hs.windows(2) {
                match (pair[0].as_str(), pair[1].as_str()) {
                    ("site", "id") => hits.push((n, "site_id".to_string())),
                    ("application", "id") => hits.push((n, "application_id".to_string())),
                    _ => {}
                }
            }
        }
        for token in line.split(|c: char| !c.is_ascii_alphanumeric()) {
            if token == "1278" {
                hits.push((n, "1278".to_string()));
            }
        }
    }
    hits
}

/// The key names of the `[features]` table of a manifest, one per line.
fn feature_keys(manifest: &str) -> String {
    let mut in_features = false;
    let mut keys = String::new();
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_features = t == "[features]";
        } else if in_features && let Some((key, _)) = t.split_once('=') {
            keys.push_str(key.trim());
            keys.push('\n');
        }
    }
    keys
}

/// All `.rs` files under `dir`, sorted.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read directory {}: {e}", dir.display()))
        .map(|e| {
            e.unwrap_or_else(|e| panic!("cannot list {}: {e}", dir.display()))
                .path()
        })
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Scans every neutral crate under `root`: `src/**/*.rs`, `build.rs` if present and the
/// `[features]` keys of `Cargo.toml`. Panics when a crate has no `src` directory or no source file
/// (a vacuous scan). Returns findings as `crate/relative/path:line: word`.
fn scan_tree(root: &Path) -> Vec<String> {
    let mut findings = Vec::new();
    for name in NEUTRAL_CRATES {
        let dir = root.join(name);
        let mut files = Vec::new();
        assert!(
            dir.join("src").is_dir(),
            "{name}: no src directory under {}",
            root.display()
        );
        rust_files(&dir.join("src"), &mut files);
        assert!(!files.is_empty(), "{name}: no source file found");
        if dir.join("build.rs").is_file() {
            files.push(dir.join("build.rs"));
        }
        let mut texts: Vec<(String, String)> = files
            .iter()
            .map(|f| {
                (
                    f.strip_prefix(root).unwrap_or(f).display().to_string(),
                    read(f),
                )
            })
            .collect();
        texts.push((
            format!("{name}/Cargo.toml"),
            feature_keys(&read(&dir.join("Cargo.toml"))),
        ));
        for (label, text) in texts {
            for (line, word) in scan(&text) {
                findings.push(format!("{label}:{line}: {word}"));
            }
        }
    }
    findings
}

fn words(source: &str) -> Vec<String> {
    scan(source).into_iter().map(|(_, w)| w).collect()
}

/// Each forbidden word is found, with its line number.
#[test]
fn given_each_forbidden_word_when_scanned_then_found_with_line_number() {
    for (text, word) in [
        ("DIS", "dis"),
        ("PDU", "pdu"),
        ("SISO", "siso"),
        ("RPR", "rpr"),
        ("1278", "1278"),
        ("site_id", "site_id"),
        ("application_id", "application_id"),
    ] {
        let src = format!("fn ok() {{}}\n// mentions {text} here\n");
        assert_eq!(scan(&src), vec![(2, word.to_string())], "{text}");
    }
}

/// Real Rust spellings: snake case, camel case, type names, upper-case constants and plurals.
#[test]
fn given_rust_spellings_when_scanned_then_every_one_matches() {
    for (text, expected) in [
        ("let pdu_len = 1;", vec!["pdu"]),
        ("fn encode_pdu() {}", vec!["pdu"]),
        ("let dis_version = 6;", vec!["dis"]),
        ("pub struct Pdu;", vec!["pdu"]),
        ("struct PduHeader {}", vec!["pdu"]),
        ("enum DisMode {}", vec!["dis"]),
        ("struct PDUHeader;", vec!["pdu"]),
        ("const DIS_MODE: u8 = 0;", vec!["dis"]),
        ("// the PDUs and DISs", vec!["pdu", "dis"]),
        ("let siteId = 1;", vec!["site_id"]),
        ("let remote_site_id = 1;", vec!["site_id"]),
        ("let ApplicationId = 1;", vec!["application_id"]),
        ("// IEEE 1278.1", vec!["1278"]),
        ("let rpr_fom = 1; let SisoRef = 2;", vec!["rpr", "siso"]),
    ] {
        assert_eq!(words(text), expected, "{text}");
    }
}

/// Look-alikes are clean.
#[test]
fn given_lookalikes_when_scanned_then_nothing_matches() {
    for text in [
        "Display dispatch distance DISCARD discard disallowed redis radis dismiss",
        "impl Display for X {} fn dispatch() {} let distance = 1;",
        "DISPLAY DISTANCE site_ident 12781 a1278 0xa1278 PDUx Dispatch",
        "let side = 1; let identity = 2; let application = 3;",
    ] {
        assert_eq!(words(text), Vec::<String>::new(), "{text}");
    }
}

fn crate_dir(root: &Path, name: &str) -> PathBuf {
    root.join(name)
}

fn write_clean_tree(root: &Path) {
    let _ = fs::remove_dir_all(root);
    for name in NEUTRAL_CRATES {
        let dir = crate_dir(root, name).join("src").join("nested");
        fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{}: {e}", "create scratch dir"));
        fs::write(dir.join("lib.rs"), "// clean\npub fn f() {}\n")
            .unwrap_or_else(|e| panic!("{}: {e}", "write scratch file"));
        fs::write(
            crate_dir(root, name).join("Cargo.toml"),
            "[package]\nname = \"x\"\n\n[features]\ndefault = []\nserver = []\n",
        )
        .unwrap_or_else(|e| panic!("{}: {e}", "write manifest"));
    }
}

/// A seeded word in a scratch tree is reported with crate, file and line; a clean tree is not.
#[test]
fn given_seeded_word_in_scratch_tree_when_scanned_then_reported() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("protocol_neutral_scratch");
    write_clean_tree(&root);
    assert_eq!(scan_tree(&root), Vec::<String>::new());

    fs::write(
        root.join("umbra-kernel/src/nested/lib.rs"),
        "// clean\npub struct Pdu;\npub const KIND: &str = \"PDU\";\n",
    )
    .unwrap_or_else(|e| panic!("{}: {e}", "seed scratch file"));
    let findings = scan_tree(&root);
    assert_eq!(findings.len(), 2, "{findings:?}");
    assert!(
        findings[0].ends_with("umbra-kernel/src/nested/lib.rs:2: pdu"),
        "{findings:?}"
    );
    assert!(
        findings[1].ends_with("umbra-kernel/src/nested/lib.rs:3: pdu"),
        "{findings:?}"
    );
}

/// `build.rs` and `[features]` keys are scanned too.
#[test]
fn given_seeded_build_script_and_feature_key_when_scanned_then_reported() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("protocol_neutral_scratch_build");
    write_clean_tree(&root);
    fs::write(
        root.join("umbra-core/build.rs"),
        "fn main() { let dis_mode = 1; }\n",
    )
    .unwrap_or_else(|e| panic!("{}: {e}", "seed build.rs"));
    fs::write(
        root.join("umbra-net/Cargo.toml"),
        "[package]\nname = \"x\"\n\n[dependencies]\ndis_rs = \"1\"\n\n[features]\ndis-codec = []\n",
    )
    .unwrap_or_else(|e| panic!("{}: {e}", "seed manifest"));
    let findings = scan_tree(&root);
    assert_eq!(
        findings,
        vec![
            "umbra-core/build.rs:1: dis".to_string(),
            "umbra-net/Cargo.toml:1: dis".to_string()
        ],
        "only [features] keys of the manifest are scanned (dependencies are check-deps' job)"
    );
}

/// A crate without a `src` directory, or without a source file, is a failure, not a pass.
#[test]
#[should_panic(expected = "no source file found")]
fn given_crate_without_sources_when_scanned_then_panics() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("protocol_neutral_scratch_empty");
    write_clean_tree(&root);
    fs::remove_file(root.join("umbra-engine/src/nested/lib.rs"))
        .unwrap_or_else(|e| panic!("{}: {e}", "remove"));
    let _ = scan_tree(&root);
}

/// A missing crate directory is a failure too.
#[test]
#[should_panic(expected = "no src directory")]
fn given_missing_crate_when_scanned_then_panics() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("protocol_neutral_scratch_missing");
    write_clean_tree(&root);
    fs::remove_dir_all(root.join("umbra-net")).unwrap_or_else(|e| panic!("{}: {e}", "remove"));
    let _ = scan_tree(&root);
}

/// The tree passes: no neutral crate mentions DIS vocabulary (ADR 0030).
#[test]
fn given_the_tree_when_scanned_then_no_neutral_crate_mentions_protocol_vocabulary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    assert_eq!(scan_tree(&root), Vec::<String>::new());
}
