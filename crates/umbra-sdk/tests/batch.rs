//! AT-3 (batch rows): `tick_batch` marshals every item, writes every status and uses only its own RNG
//! token. Requirements: REQ-B-4, REQ-P-7, REQ-K-4, R-B2, R-B4. Step 4 of M4 (WI-7 stub).

mod support;

use support::{Draw, Pushed, Rig, draws, pushed, reset};
use umbra_sdk::abi::{
    UMBRA_ERR_PANIC, UMBRA_OK, UMBRA_STATUS_FAILURE, UMBRA_STATUS_RUNNING, UMBRA_STATUS_SUCCESS,
};

/// R-B2: after a clean call no status is left `UNSET`; the numbering matches the ABI.
#[test]
fn statuses_written_for_every_item() {
    reset();
    let mut rig = Rig::new(&[(1, 11), (2, 22), (3, 33)], "{}");

    let (code, _) = rig.tick(0);

    assert_eq!(code, UMBRA_OK);
    // The test plugin returns Failure, Running, Success for entity ids 1, 2, 3 (id % 3).
    assert_eq!(
        rig.statuses,
        [
            UMBRA_STATUS_FAILURE,
            UMBRA_STATUS_RUNNING,
            UMBRA_STATUS_SUCCESS
        ]
    );
}

/// Intents per item as `(kind, value bits)`, in push order.
fn per_item(all: &[Pushed], items: usize) -> Vec<Vec<(u32, u64)>> {
    let mut out = vec![Vec::new(); items];
    for p in all {
        out[p.item].push((p.kind, p.value.to_bits()));
    }
    out
}

/// ADR 0020: one call over N items equals N calls over one item each (statuses, intents per item,
/// draws per token).
#[test]
fn tick_batch_equals_single_ticks() {
    let members = [(1_u64, 11_u64), (2, 22), (3, 33), (4, 44)];

    reset();
    let mut batched = Rig::new(&members, "{}");
    let mut batched_statuses = Vec::new();
    for t in 0..5 {
        assert_eq!(batched.tick(t).0, UMBRA_OK);
        batched_statuses.push(batched.statuses.clone());
    }
    let batched_intents = per_item(&pushed(), members.len());
    let batched_draws: Vec<Draw> = draws();

    reset();
    let mut singles: Vec<Rig> = members.iter().map(|m| Rig::new(&[*m], "{}")).collect();
    let mut single_statuses = Vec::new();
    let mut single_intents = vec![Vec::new(); members.len()];
    for t in 0..5 {
        let mut row = Vec::new();
        for (n, r) in singles.iter_mut().enumerate() {
            let before = pushed().len();
            assert_eq!(r.tick(t).0, UMBRA_OK);
            row.push(r.statuses[0]);
            for p in &pushed()[before..] {
                single_intents[n].push((p.kind, p.value.to_bits()));
            }
        }
        single_statuses.push(row);
    }

    assert_eq!(batched_statuses, single_statuses);
    assert_eq!(batched_intents, single_intents);
    let by_token = |all: &[Draw]| {
        let mut m = std::collections::BTreeMap::<u64, Vec<u64>>::new();
        for d in all {
            m.entry(d.token).or_default().push(d.value);
        }
        m
    };
    assert_eq!(by_token(&batched_draws), by_token(&draws()));
}

/// R-B4, REQ-K-4: the plugin draws only through the host API, with the token of the item it is ticking
/// and the image's host context.
#[test]
fn rng_only_through_host_api_with_own_token() {
    reset();
    let mut rig = Rig::new(&[(1, 11), (2, 22)], "{}");

    let (code, _) = rig.tick(0);
    let (code2, _) = rig.tick(1);

    assert_eq!((code, code2), (UMBRA_OK, UMBRA_OK));
    let got = draws();
    assert_ne!(got.len(), 0);
    assert!(got.iter().all(|d| d.host_ctx == support::HOST_CTX));
    let tokens: Vec<u64> = got.iter().map(|d| d.token).collect();
    assert!(
        tokens.iter().all(|t| *t == 11 || *t == 22),
        "only the items' own tokens: {tokens:?}"
    );
    // Tick 0 ticks item 0 fully before item 1: the token sequence is non-decreasing per tick block.
    let first_22 = tokens.iter().position(|t| *t == 22).expect("item 2 drew");
    assert!(tokens[..first_22].iter().all(|t| *t == 11));
    // Intents carry the index of the item that produced them.
    assert!(pushed().iter().any(|p| p.item == 1));
}

/// R-M1: a fault in one item's tick makes the whole call report the code (the host then discards the
/// batch, ADR 0020); the SDK does not continue past a panic.
#[test]
fn panic_in_one_item_is_reported_for_the_call() {
    reset();
    let mut rig = Rig::new(&[(1, 11)], "{\"panic\":\"tick\"}");

    let (code, msg) = rig.tick(0);

    assert_eq!(code, UMBRA_ERR_PANIC);
    assert!(msg.is_some());
}
