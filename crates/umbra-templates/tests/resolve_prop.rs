//! AT-2 property: `inheritance_resolution`. Requirements: REQ-T-2, spec section 15 (template
//! inheritance property test), REQ-K-4.
//!
//! Random forests of 1-30 templates with random `extends` (including self loops and longer cycles)
//! and random parameter overrides. Properties: resolution terminates; there is exactly one cycle
//! diagnostic per cycle and nothing else (children of cycles and templates outside them get no
//! diagnostic); for acyclic forests every template's params equal the root-first fold of its chain.

#![forbid(unsafe_code)]

mod common;

use std::collections::BTreeSet;

use proptest::prelude::*;
use serde_json::{Map, Value};

use common::{errors, ok, templates, tid};
use umbra_templates::SourceFile;

/// One generated template: the index it extends (modulo the forest size) and its x, y, z params.
type Spec = (Option<usize>, [Option<u8>; 3]);

fn id_of(i: usize) -> String {
    format!("node.t{i:02}")
}

fn source_of(i: usize, extends: Option<usize>, params: [Option<u8>; 3]) -> SourceFile {
    let fields: Vec<String> = ["x", "y", "z"]
        .iter()
        .zip(params)
        .filter_map(|(k, v)| v.map(|v| format!("{k} = {v}.0")))
        .collect();
    let mut lines = vec![
        "schema_version = 1".to_string(),
        format!("id = \"{}\"", id_of(i)),
    ];
    lines.extend(extends.map(|parent| format!("extends = \"{}\"", id_of(parent))));
    lines.push("[motion]".to_string());
    lines.push("model = \"stub.motion.gamma\"".to_string());
    lines.push(format!("params = {{ {} }}", fields.join(", ")));
    SourceFile::new(format!("t/{i:02}.toml"), lines.join("\n") + "\n")
}

/// The sources and the `extends` indices. Acyclic forests only extend lower indices.
fn build(specs: &[Spec], acyclic: bool) -> (Vec<SourceFile>, Vec<Option<usize>>) {
    let n = specs.len();
    let extends: Vec<Option<usize>> = specs
        .iter()
        .enumerate()
        .map(|(i, (e, _))| {
            let raw = (*e)?;
            if acyclic {
                (i > 0).then(|| raw % i)
            } else {
                Some(raw % n)
            }
        })
        .collect();
    let files = specs
        .iter()
        .enumerate()
        .map(|(i, (_, p))| source_of(i, extends[i], *p))
        .collect();
    (files, extends)
}

/// Number of distinct cycles in a functional graph `i -> extends[i]`.
fn count_cycles(extends: &[Option<usize>]) -> usize {
    let n = extends.len();
    let mut state = vec![0_u8; n]; // 0 new, 1 on the current path, 2 done
    let mut cycles = 0;
    for start in 0..n {
        let mut path = Vec::new();
        let mut at = Some(start);
        while let Some(i) = at {
            match state[i] {
                0 => {
                    state[i] = 1;
                    path.push(i);
                    at = extends[i];
                }
                1 => {
                    cycles += 1;
                    break;
                }
                _ => break,
            }
        }
        for i in path {
            state[i] = 2;
        }
    }
    cycles
}

/// The root-first fold of the params along the chain of `i`.
fn folded(i: usize, extends: &[Option<usize>], specs: &[Spec]) -> Value {
    let mut chain = vec![i];
    let mut at = extends[i];
    while let Some(p) = at {
        chain.push(p);
        at = extends[p];
    }
    let mut out = Map::new();
    for &t in chain.iter().rev() {
        for (k, v) in ["x", "y", "z"].iter().zip(specs[t].1) {
            if let Some(v) = v {
                out.insert((*k).to_string(), Value::from(f64::from(v)));
            }
        }
    }
    Value::Object(out)
}

fn spec_strategy() -> impl Strategy<Value = Spec> {
    (
        proptest::option::weighted(0.75, 0_usize..64),
        [
            proptest::option::of(1_u8..10),
            proptest::option::of(1_u8..10),
            proptest::option::of(1_u8..10),
        ],
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    /// Any forest, cycles included: resolution terminates, reports exactly one `inheritance_cycle`
    /// per cycle, and nothing else.
    #[test]
    fn inheritance_resolution_reports_one_cycle_error_per_cycle_and_no_cascade(
        specs in proptest::collection::vec(spec_strategy(), 1..=30)
    ) {
        let (files, extends) = build(&specs, false);
        let cycles = count_cycles(&extends);

        let result = templates(&files);

        if cycles == 0 {
            prop_assert!(result.is_ok(), "an acyclic forest must resolve");
        } else {
            let found = errors(result);
            prop_assert_eq!(found.len(), cycles);
            prop_assert!(found.iter().all(|d| d.code.as_str() == "inheritance_cycle"));
            // One error per cycle means the reported `extends` locations are all distinct.
            let places: BTreeSet<_> = found
                .iter()
                .filter_map(|d| d.location.as_ref().map(|l| (l.path.clone(), l.line, l.column)))
                .collect();
            prop_assert_eq!(places.len(), cycles);
        }
    }

    /// Acyclic forests: every template's params are the root-first fold of its chain, and its chain
    /// lists itself first and the root last.
    #[test]
    fn inheritance_resolution_folds_params_root_first_along_the_chain(
        specs in proptest::collection::vec(spec_strategy(), 1..=30)
    ) {
        let (files, extends) = build(&specs, true);

        let set = ok(templates(&files));

        prop_assert_eq!(set.len(), specs.len());
        for i in 0..specs.len() {
            let t = set.get(&tid(&id_of(i))).unwrap_or_else(|| panic!("missing {i}"));
            prop_assert_eq!(&t.motion.params, &folded(i, &extends, &specs));
            prop_assert_eq!(t.chain.first(), Some(&tid(&id_of(i))));
            let mut root = i;
            while let Some(p) = extends[root] {
                root = p;
            }
            prop_assert_eq!(t.chain.last(), Some(&tid(&id_of(root))));
        }
    }

    /// The file order does not matter (REQ-K-4): a rotated and a reversed input give equal results.
    #[test]
    fn inheritance_resolution_is_independent_of_file_order(
        specs in proptest::collection::vec(spec_strategy(), 1..=12),
        shift in 0_usize..12,
    ) {
        let (files, _) = build(&specs, false);
        let mut rotated = files.clone();
        rotated.rotate_left(shift % files.len());
        let mut reversed = files.clone();
        reversed.reverse();

        let base = templates(&files);

        prop_assert_eq!(&base, &templates(&rotated));
        prop_assert_eq!(&base, &templates(&reversed));
    }
}
